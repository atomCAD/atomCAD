use super::text_format::{parse_diff_text, serialize_diff};
use super::types::*;
use crate::api::common_api_types::SelectModifier;
use crate::api::structure_designer::structure_designer_api_types::APIAtomEditTool;
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::atomic_structure::{AtomicStructure, BondReference};
use crate::crystolecule::atomic_structure_diff::{
    AtomSource, DiffProvenance, apply_diff, enrich_diff_with_base_bonds,
};
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{NodeType, Parameter};
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use crate::util::transform::Transform;
use glam::f64::{DQuat, DVec3};
use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::Mutex;

use crate::structure_designer::serialization::atom_edit_data_serialization::{
    SerializableAtomEditData, atom_edit_data_to_serializable, serializable_to_atom_edit_data,
};

// --- Main data struct ---

/// Data for the `atom_edit` node.
///
/// Uses a diff-based design: instead of a command stack, edits are represented as a single
/// `AtomicStructure` diff. Evaluation applies the diff to the input via `apply_diff()`.
#[flutter_rust_bridge::frb(ignore)]
#[derive(Debug)]
pub struct AtomEditData {
    // Persistent (serialized to .cnnd)
    /// The diff structure (is_diff = true)
    pub diff: AtomicStructure,
    /// When true, output the diff itself instead of the applied result
    pub output_diff: bool,
    /// When true + output_diff, render anchor arrows in diff view
    pub show_anchor_arrows: bool,
    /// When true + output_diff, include base bonds between matched diff atoms in diff output
    pub include_base_bonds_in_diff: bool,
    /// Positional matching tolerance in Angstroms
    pub tolerance: f64,
    /// When true + result view, return error if any diff diagnostics are non-zero
    pub error_on_stale_entries: bool,

    // Transient (NOT serialized)
    /// Current selection state
    pub selection: AtomEditSelection,
    /// Current editing tool
    pub active_tool: AtomEditTool,
    /// Last known diff stats (updated during eval, used by get_subtitle)
    last_stats: Option<crate::crystolecule::atomic_structure_diff::DiffStats>,
    /// Cached input molecule for interactive editing performance.
    /// When present, reused instead of re-evaluating upstream.
    /// Cleared by `clear_input_cache()` when upstream may have changed.
    cached_input: Mutex<Option<AtomicStructure>>,
}

impl Default for AtomEditData {
    fn default() -> Self {
        Self::new()
    }
}

impl AtomEditData {
    pub fn new() -> Self {
        Self {
            diff: AtomicStructure::new_diff(),
            output_diff: false,
            show_anchor_arrows: false,
            include_base_bonds_in_diff: true,
            tolerance: DEFAULT_TOLERANCE,
            error_on_stale_entries: false,
            selection: AtomEditSelection::new(),
            active_tool: AtomEditTool::Default(DefaultToolState {
                replacement_atomic_number: 6, // Default to carbon
                interaction_state: DefaultToolInteractionState::default(),
                show_gadget: false,
            }),
            last_stats: None,
            cached_input: Mutex::new(None),
        }
    }

    /// Creates an AtomEditData from deserialized data.
    /// Used by the serialization module to restore state from .cnnd files.
    pub fn from_deserialized(
        diff: AtomicStructure,
        output_diff: bool,
        show_anchor_arrows: bool,
        include_base_bonds_in_diff: bool,
        tolerance: f64,
        error_on_stale_entries: bool,
    ) -> Self {
        Self {
            diff,
            output_diff,
            show_anchor_arrows,
            include_base_bonds_in_diff,
            tolerance,
            error_on_stale_entries,
            selection: AtomEditSelection::new(),
            active_tool: AtomEditTool::Default(DefaultToolState {
                replacement_atomic_number: 6,
                interaction_state: DefaultToolInteractionState::default(),
                show_gadget: false,
            }),
            last_stats: None,
            cached_input: Mutex::new(None),
        }
    }

    // --- Direct diff mutation methods ---

    /// Add an atom to the diff at the given position.
    /// Returns the new atom's ID in the diff.
    pub fn add_atom_to_diff(&mut self, atomic_number: i16, position: DVec3) -> u32 {
        self.selection.clear_bonds();
        self.diff.add_atom(atomic_number, position)
    }

    /// Add a delete marker at the given position.
    /// Returns the delete marker's ID in the diff.
    pub fn mark_for_deletion(&mut self, match_position: DVec3) -> u32 {
        self.selection.clear_bonds();
        self.diff.add_atom(
            crate::crystolecule::atomic_structure::DELETED_SITE_ATOMIC_NUMBER,
            match_position,
        )
    }

    /// Add/update an atom in the diff with a new atomic number at the given position.
    /// Returns the atom's ID in the diff.
    pub fn replace_in_diff(&mut self, match_position: DVec3, new_atomic_number: i16) -> u32 {
        self.selection.clear_bonds();
        self.diff.add_atom(new_atomic_number, match_position)
    }

    /// Move an atom in the diff to a new position.
    /// Sets anchor to the current position if not already set (first move).
    pub fn move_in_diff(&mut self, atom_id: u32, new_position: DVec3) {
        self.selection.clear_bonds();
        if let Some(atom) = self.diff.get_atom(atom_id) {
            // Set anchor to current position if not already set
            if !self.diff.has_anchor_position(atom_id) {
                self.diff.set_anchor_position(atom_id, atom.position);
            }
        }
        self.diff.set_atom_position(atom_id, new_position);
    }

    /// Add a bond between two atoms in the diff.
    pub fn add_bond_in_diff(&mut self, atom_id1: u32, atom_id2: u32, order: u8) {
        self.selection.clear_bonds();
        self.diff.add_bond_checked(atom_id1, atom_id2, order);
    }

    /// Add a bond delete marker between two atoms in the diff.
    /// Ensures both atoms are present in the diff (adds identity entries if needed).
    pub fn delete_bond_in_diff(&mut self, atom_id1: u32, atom_id2: u32) {
        self.selection.clear_bonds();
        self.diff.add_bond(
            atom_id1,
            atom_id2,
            crate::crystolecule::atomic_structure::BOND_DELETED,
        );
    }

    /// Get a clone of the cached input structure (if available).
    /// Used by bond order change operations to resolve result-space IDs.
    pub fn get_cached_input(&self) -> Option<AtomicStructure> {
        self.cached_input.lock().ok().and_then(|g| g.clone())
    }

    /// Remove an atom from the diff entirely (and its anchor if any).
    pub fn remove_from_diff(&mut self, diff_atom_id: u32) {
        self.selection.clear_bonds();
        self.diff.delete_atom(diff_atom_id);
        self.diff.remove_anchor_position(diff_atom_id);
    }

    // --- Tool management ---

    pub fn get_active_tool(&self) -> APIAtomEditTool {
        match &self.active_tool {
            AtomEditTool::Default(_) => APIAtomEditTool::Default,
            AtomEditTool::AddAtom(_) => APIAtomEditTool::AddAtom,
            AtomEditTool::AddBond(_) => APIAtomEditTool::AddBond,
        }
    }

    pub fn set_active_tool(&mut self, api_tool: APIAtomEditTool) {
        // Reset interaction state if switching away from Default tool mid-interaction
        if let AtomEditTool::Default(ref mut state) = self.active_tool {
            state.interaction_state = DefaultToolInteractionState::Idle;
        }
        // Cancel guided placement if switching away from AddAtom tool
        // (no special action needed — the new tool state replaces the old one)
        self.active_tool = match api_tool {
            APIAtomEditTool::Default => AtomEditTool::Default(DefaultToolState {
                replacement_atomic_number: 6,
                interaction_state: DefaultToolInteractionState::default(),
                show_gadget: false,
            }),
            APIAtomEditTool::AddAtom => {
                AtomEditTool::AddAtom(AddAtomToolState::Idle { atomic_number: 6 })
            }
            APIAtomEditTool::AddBond => AtomEditTool::AddBond(AddBondToolState {
                bond_order: crate::crystolecule::atomic_structure::BOND_SINGLE,
                interaction_state: AddBondInteractionState::default(),
                last_atom_id: None,
            }),
        }
    }

    pub fn set_default_tool_atomic_number(&mut self, replacement_atomic_number: i16) -> bool {
        match &mut self.active_tool {
            AtomEditTool::Default(state) => {
                state.replacement_atomic_number = replacement_atomic_number;
                true
            }
            _ => false,
        }
    }

    pub fn set_add_atom_tool_atomic_number(&mut self, atomic_number: i16) -> bool {
        match &mut self.active_tool {
            AtomEditTool::AddAtom(state) => {
                // Reset to Idle with new atomic number (cancel any guided placement)
                *state = AddAtomToolState::Idle { atomic_number };
                true
            }
            _ => false,
        }
    }

    // --- Decoration helpers ---

    /// Apply guided placement decoration (anchor highlight + guide visuals) to an output structure.
    ///
    /// In diff view (`provenance` is None), anchor_atom_id is used directly.
    /// In result view (`provenance` is Some), anchor_atom_id is mapped through provenance.
    fn apply_guided_placement_decoration(
        &self,
        output: &mut AtomicStructure,
        provenance: Option<&crate::crystolecule::atomic_structure_diff::DiffProvenance>,
    ) {
        use crate::crystolecule::atomic_structure::AtomDisplayState;
        use crate::crystolecule::atomic_structure::atomic_structure_decorator::{
            GuidePlacementVisuals, WireframeRingVisuals, WireframeSphereVisuals,
        };

        // Helper to resolve anchor_atom_id to output atom ID
        let resolve_anchor = |anchor_atom_id: u32| -> Option<u32> {
            match provenance {
                None => Some(anchor_atom_id), // diff view: direct
                Some(prov) => prov.diff_to_result.get(&anchor_atom_id).copied(),
            }
        };

        match &self.active_tool {
            AtomEditTool::AddAtom(AddAtomToolState::GuidedPlacement {
                anchor_atom_id,
                guide_dots,
                ..
            }) => {
                if let Some(output_id) = resolve_anchor(*anchor_atom_id) {
                    output
                        .decorator_mut()
                        .set_atom_display_state(output_id, AtomDisplayState::Marked);
                    if let Some(anchor_atom) = output.get_atom(output_id) {
                        output.decorator_mut().guide_placement_visuals =
                            Some(GuidePlacementVisuals {
                                anchor_pos: anchor_atom.position,
                                guide_dots: guide_dots.clone(),
                                wireframe_sphere: None,
                                wireframe_ring: None,
                            });
                    }
                }
            }
            AtomEditTool::AddAtom(AddAtomToolState::GuidedFreeSphere {
                anchor_atom_id,
                center,
                radius,
                preview_position,
                ..
            }) => {
                if let Some(output_id) = resolve_anchor(*anchor_atom_id) {
                    output
                        .decorator_mut()
                        .set_atom_display_state(output_id, AtomDisplayState::Marked);
                    if let Some(anchor_atom) = output.get_atom(output_id) {
                        // Build guide dots from preview position (if cursor is on sphere)
                        let preview_dots: Vec<crate::crystolecule::guided_placement::GuideDot> =
                            preview_position
                                .iter()
                                .map(|pos| crate::crystolecule::guided_placement::GuideDot {
                                    position: *pos,
                                    dot_type:
                                        crate::crystolecule::guided_placement::GuideDotType::Primary,
                                })
                                .collect();

                        output.decorator_mut().guide_placement_visuals =
                            Some(GuidePlacementVisuals {
                                anchor_pos: anchor_atom.position,
                                guide_dots: preview_dots,
                                wireframe_sphere: Some(WireframeSphereVisuals {
                                    center: *center,
                                    radius: *radius,
                                    preview_position: *preview_position,
                                }),
                                wireframe_ring: None,
                            });
                    }
                }
            }
            AtomEditTool::AddAtom(AddAtomToolState::GuidedFreeRing {
                anchor_atom_id,
                ring_center,
                ring_normal,
                ring_radius,
                preview_positions,
                ..
            }) => {
                if let Some(output_id) = resolve_anchor(*anchor_atom_id) {
                    output
                        .decorator_mut()
                        .set_atom_display_state(output_id, AtomDisplayState::Marked);
                    if let Some(anchor_atom) = output.get_atom(output_id) {
                        // Build guide dots from preview positions (sp3: 3 dots, sp2: 2 dots)
                        let preview_dots: Vec<crate::crystolecule::guided_placement::GuideDot> =
                            preview_positions
                                .iter()
                                .flat_map(|positions| positions.iter())
                                .map(|pos| crate::crystolecule::guided_placement::GuideDot {
                                    position: *pos,
                                    dot_type:
                                        crate::crystolecule::guided_placement::GuideDotType::Primary,
                                })
                                .collect();

                        output.decorator_mut().guide_placement_visuals =
                            Some(GuidePlacementVisuals {
                                anchor_pos: anchor_atom.position,
                                guide_dots: preview_dots,
                                wireframe_sphere: None,
                                wireframe_ring: Some(WireframeRingVisuals {
                                    center: *ring_center,
                                    normal: *ring_normal,
                                    radius: *ring_radius,
                                }),
                            });
                    }
                }
            }
            _ => {}
        }
    }

    // --- Core mutation methods (testable without StructureDesigner) ---

    /// Convert a matched diff atom to a delete marker.
    ///
    /// The delete marker is placed at the match position (anchor if present,
    /// else atom position) so it matches the same base atom during apply_diff.
    pub fn convert_to_delete_marker(&mut self, diff_atom_id: u32) {
        let match_position = {
            let anchor = self.diff.anchor_position(diff_atom_id).copied();
            match anchor {
                Some(pos) => pos,
                None => match self.diff.get_atom(diff_atom_id) {
                    Some(atom) => atom.position,
                    None => return,
                },
            }
        };

        self.remove_from_diff(diff_atom_id);
        self.mark_for_deletion(match_position);
    }

    /// Apply deletion in result view. Called by `delete_selected_in_result_view`
    /// after gathering positions and provenance info from StructureDesigner.
    pub fn apply_delete_result_view(
        &mut self,
        base_atoms: &[(u32, DVec3)],
        diff_atoms: &[(u32, bool)],
        bonds: &[BondDeletionInfo],
    ) {
        // Delete base atoms (add delete markers)
        for (base_id, position) in base_atoms {
            self.mark_for_deletion(*position);
            self.selection.selected_base_atoms.remove(base_id);
        }

        // Delete diff atoms
        for (diff_id, is_pure_addition) in diff_atoms {
            if *is_pure_addition {
                self.remove_from_diff(*diff_id);
            } else {
                self.convert_to_delete_marker(*diff_id);
            }
            self.selection.selected_diff_atoms.remove(diff_id);
        }

        // Delete bonds (add bond delete markers)
        for info in bonds {
            let actual_a = match info.diff_id_a {
                Some(id) => id,
                None => match info.identity_a {
                    Some((an, pos)) => self.diff.add_atom(an, pos),
                    None => continue,
                },
            };
            let actual_b = match info.diff_id_b {
                Some(id) => id,
                None => match info.identity_b {
                    Some((an, pos)) => self.diff.add_atom(an, pos),
                    None => continue,
                },
            };
            self.delete_bond_in_diff(actual_a, actual_b);
        }

        self.selection.selected_bonds.clear();
        self.selection.selection_transform = None;
    }

    /// Apply deletion in diff view (reversal semantics).
    pub fn apply_delete_diff_view(
        &mut self,
        diff_atoms: &[(u32, DiffAtomKind)],
        bonds: &[BondReference],
    ) {
        for (diff_id, kind) in diff_atoms {
            match kind {
                // Delete marker → remove from diff (un-deletes the base atom)
                DiffAtomKind::DeleteMarker => {
                    self.remove_from_diff(*diff_id);
                }
                // Moved/replaced base atom → convert to delete marker
                DiffAtomKind::MatchedBase => {
                    self.convert_to_delete_marker(*diff_id);
                }
                // Pure addition → remove entirely
                DiffAtomKind::PureAddition => {
                    self.remove_from_diff(*diff_id);
                }
            }
            self.selection.selected_diff_atoms.remove(diff_id);
        }

        // Bonds in diff view: remove the bond from the diff entirely
        for bond_ref in bonds {
            self.diff.delete_bond(bond_ref);
        }

        self.selection.selected_bonds.clear();
        self.selection.selection_transform = None;
    }

    /// Apply element replacement to selected atoms.
    pub fn apply_replace(&mut self, atomic_number: i16, base_atoms: &[(u32, DVec3)]) {
        // Replace diff atoms (update atomic_number in place)
        let diff_ids: Vec<u32> = self.selection.selected_diff_atoms.iter().copied().collect();
        for diff_id in &diff_ids {
            self.diff.set_atomic_number(*diff_id, atomic_number);
        }

        // Replace base atoms (add to diff with new element)
        for (base_id, position) in base_atoms {
            let new_diff_id = self.replace_in_diff(*position, atomic_number);
            self.selection.selected_base_atoms.remove(base_id);
            self.selection.selected_diff_atoms.insert(new_diff_id);
        }

        self.selection.clear_bonds();
    }

    /// Apply a relative transform to selected atoms.
    pub fn apply_transform(&mut self, relative: &Transform, base_atoms: &[(u32, i16, DVec3)]) {
        // Transform existing diff atoms (update position, keep anchor)
        let diff_ids: Vec<u32> = self.selection.selected_diff_atoms.iter().copied().collect();
        for diff_id in diff_ids {
            let new_position = if let Some(atom) = self.diff.get_atom(diff_id) {
                relative.apply_to_position(&atom.position)
            } else {
                continue;
            };
            self.diff.set_atom_position(diff_id, new_position);
        }

        // Add base atoms to diff with anchors at old positions
        for (base_id, atomic_number, old_position) in base_atoms {
            let new_position = relative.apply_to_position(old_position);
            let new_diff_id = self.diff.add_atom(*atomic_number, new_position);
            self.diff.set_anchor_position(new_diff_id, *old_position);
            self.selection.selected_base_atoms.remove(base_id);
            self.selection.selected_diff_atoms.insert(new_diff_id);
        }

        // Update selection transform algebraically (no need to re-eval)
        if let Some(ref current_transform) = self.selection.selection_transform {
            self.selection.selection_transform = Some(current_transform.apply_to_new(relative));
        }
        self.selection.clear_bonds();
    }
}

impl NodeData for AtomEditData {
    fn provide_gadget(
        &self,
        structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        use super::atom_edit_gadget::AtomEditSelectionGadget;

        // Gadget is only shown when the Default tool has show_gadget enabled.
        let show = match &self.active_tool {
            AtomEditTool::Default(state) => state.show_gadget,
            _ => false,
        };
        if !show || !self.selection.has_selected_atoms() {
            return None;
        }

        // Use existing selection_transform centroid as gadget position.
        let center = self.selection.selection_transform.as_ref()?.translation;

        // Gather diff atom positions directly from the diff.
        let mut diff_atom_positions: Vec<(u32, DVec3)> = Vec::new();
        for &diff_id in &self.selection.selected_diff_atoms {
            if let Some(atom) = self.diff.get_atom(diff_id) {
                diff_atom_positions.push((diff_id, atom.position));
            }
        }

        // Gather base atom info (needs eval cache for provenance → result positions).
        let mut base_atoms_info: Vec<(u32, i16, DVec3)> = Vec::new();
        if !self.selection.selected_base_atoms.is_empty() && !self.output_diff {
            if let Some(eval_cache) = structure_designer.get_selected_node_eval_cache() {
                if let Some(cache) = eval_cache.downcast_ref::<AtomEditEvalCache>() {
                    if let Some(result) =
                        structure_designer.get_atomic_structure_from_selected_node()
                    {
                        for &base_id in &self.selection.selected_base_atoms {
                            if let Some(&result_id) = cache.provenance.base_to_result.get(&base_id)
                            {
                                if let Some(atom) = result.get_atom(result_id) {
                                    base_atoms_info.push((
                                        base_id,
                                        atom.atomic_number,
                                        atom.position,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }

        Some(Box::new(AtomEditSelectionGadget::new(
            center,
            diff_atom_positions,
            base_atoms_info,
        )))
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        network_evaluator: &NetworkEvaluator,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &crate::structure_designer::node_type_registry::NodeTypeRegistry,
        decorate: bool,
        context: &mut crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext,
    ) -> NetworkResult {
        // Use cached input if available (populated during previous eval,
        // cleared by clear_input_cache() when upstream may have changed).
        let input_structure = if let Some(cached) = self
            .cached_input
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
        {
            cached
        } else {
            let input_val =
                network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 0);
            if let NetworkResult::Error(_) = input_val {
                return input_val;
            }
            let structure = match input_val {
                NetworkResult::Atomic(s) => s,
                _ => AtomicStructure::new(),
            };
            if let Ok(mut guard) = self.cached_input.lock() {
                *guard = Some(structure.clone());
            }
            structure
        };

        {
            if self.output_diff {
                // Output the diff itself for visualization/debugging
                let mut diff_clone = self.diff.clone();
                if self.include_base_bonds_in_diff {
                    enrich_diff_with_base_bonds(&mut diff_clone, &input_structure, self.tolerance);
                }
                diff_clone.decorator_mut().show_anchor_arrows = self.show_anchor_arrows;
                if decorate {
                    diff_clone.decorator_mut().from_selected_node = true;

                    // Apply diff atom selection directly (no provenance needed —
                    // diff atom IDs ARE the output atom IDs in diff view)
                    for &diff_id in &self.selection.selected_diff_atoms {
                        diff_clone.set_atom_selected(diff_id, true);
                    }

                    // Apply bond selection
                    for bond_ref in &self.selection.selected_bonds {
                        diff_clone.decorator_mut().select_bond(bond_ref);
                    }

                    // Apply selection transform
                    if let Some(ref transform) = self.selection.selection_transform {
                        diff_clone.decorator_mut().selection_transform = Some(transform.clone());
                    }

                    // Mark guided placement anchor and store guide visuals
                    self.apply_guided_placement_decoration(&mut diff_clone, None);
                }
                return NetworkResult::Atomic(diff_clone);
            }

            // Apply the diff to the input
            let diff_result = apply_diff(&input_structure, &self.diff, self.tolerance);

            // Error on stale entries: if enabled and any diagnostics are non-zero, return error
            if self.error_on_stale_entries {
                let s = &diff_result.stats;
                if s.orphaned_tracked_atoms > 0
                    || s.unmatched_delete_markers > 0
                    || s.orphaned_bonds > 0
                {
                    let mut parts = Vec::new();
                    if s.orphaned_tracked_atoms > 0 {
                        parts.push(format!(
                            "{} orphaned tracked atom(s)",
                            s.orphaned_tracked_atoms
                        ));
                    }
                    if s.unmatched_delete_markers > 0 {
                        parts.push(format!(
                            "{} unmatched delete marker(s)",
                            s.unmatched_delete_markers
                        ));
                    }
                    if s.orphaned_bonds > 0 {
                        parts.push(format!("{} orphaned bond(s)", s.orphaned_bonds));
                    }
                    return NetworkResult::Error(format!("Stale entries: {}", parts.join(", ")));
                }
            }

            let mut result = diff_result.result;

            // Apply selection to result (mark atoms as selected for rendering)
            if decorate {
                result.decorator_mut().from_selected_node = true;

                // Apply atom selection via provenance maps
                for &base_id in &self.selection.selected_base_atoms {
                    if let Some(&result_id) = diff_result.provenance.base_to_result.get(&base_id) {
                        result.set_atom_selected(result_id, true);
                    }
                    // Silently skip stale IDs
                }

                for &diff_id in &self.selection.selected_diff_atoms {
                    if let Some(&result_id) = diff_result.provenance.diff_to_result.get(&diff_id) {
                        result.set_atom_selected(result_id, true);
                    }
                    // Silently skip stale IDs
                }

                // Apply bond selection
                for bond_ref in &self.selection.selected_bonds {
                    result.decorator_mut().select_bond(bond_ref);
                }

                // Apply selection transform
                if let Some(ref transform) = self.selection.selection_transform {
                    result.decorator_mut().selection_transform = Some(transform.clone());
                }

                // Mark AddBond tool's source atom if in Pending or Dragging state
                if let AtomEditTool::AddBond(state) = &self.active_tool {
                    let mark_diff_id = match &state.interaction_state {
                        AddBondInteractionState::Pending { hit_atom_id, .. } => Some(*hit_atom_id),
                        AddBondInteractionState::Dragging { source_atom_id, .. } => {
                            Some(*source_atom_id)
                        }
                        AddBondInteractionState::Idle => state.last_atom_id,
                    };
                    if let Some(diff_id) = mark_diff_id {
                        // Map diff atom ID to result atom ID
                        if let Some(&result_id) =
                            diff_result.provenance.diff_to_result.get(&diff_id)
                        {
                            result.decorator_mut().set_atom_display_state(
                                result_id,
                                crate::crystolecule::atomic_structure::AtomDisplayState::Marked,
                            );
                        }
                    }
                }

                // Mark guided placement anchor and store guide visuals
                self.apply_guided_placement_decoration(&mut result, Some(&diff_result.provenance));
            }

            // Store provenance and stats in eval cache for root-level evaluations
            if network_stack.len() == 1 {
                let eval_cache = AtomEditEvalCache {
                    provenance: diff_result.provenance,
                    stats: diff_result.stats,
                };
                context.selected_node_eval_cache = Some(Box::new(eval_cache));
            }

            NetworkResult::Atomic(result)
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(AtomEditData {
            diff: self.diff.clone(),
            output_diff: self.output_diff,
            show_anchor_arrows: self.show_anchor_arrows,
            include_base_bonds_in_diff: self.include_base_bonds_in_diff,
            tolerance: self.tolerance,
            error_on_stale_entries: self.error_on_stale_entries,
            selection: self.selection.clone(),
            active_tool: match &self.active_tool {
                AtomEditTool::Default(state) => AtomEditTool::Default(DefaultToolState {
                    replacement_atomic_number: state.replacement_atomic_number,
                    interaction_state: DefaultToolInteractionState::default(),
                    show_gadget: state.show_gadget,
                }),
                AtomEditTool::AddAtom(state) => AtomEditTool::AddAtom(AddAtomToolState::Idle {
                    atomic_number: state.atomic_number(),
                }),
                AtomEditTool::AddBond(state) => AtomEditTool::AddBond(AddBondToolState {
                    bond_order: state.bond_order,
                    interaction_state: AddBondInteractionState::default(),
                    last_atom_id: state.last_atom_id,
                }),
            },
            last_stats: self.last_stats.clone(),
            cached_input: Mutex::new(None),
        })
    }

    fn clear_input_cache(&self) {
        if let Ok(mut guard) = self.cached_input.lock() {
            *guard = None;
        }
    }

    fn get_subtitle(&self, _connected_input_pins: &HashSet<String>) -> Option<String> {
        // Use last known stats if available (updated during eval)
        if let Some(stats) = &self.last_stats {
            let mut parts = Vec::new();
            if stats.atoms_added > 0 {
                parts.push(format!("+{}", stats.atoms_added));
            }
            if stats.atoms_deleted > 0 {
                parts.push(format!("-{}", stats.atoms_deleted));
            }
            if stats.atoms_modified > 0 {
                parts.push(format!("~{}", stats.atoms_modified));
            }
            if parts.is_empty() {
                Some("no changes".to_string())
            } else {
                Some(parts.join(", "))
            }
        } else if self.diff.get_num_of_atoms() > 0 {
            Some(format!("diff: {} atoms", self.diff.get_num_of_atoms()))
        } else {
            None
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            (
                "diff".to_string(),
                TextValue::String(serialize_diff(&self.diff)),
            ),
            ("output_diff".to_string(), TextValue::Bool(self.output_diff)),
            (
                "show_anchor_arrows".to_string(),
                TextValue::Bool(self.show_anchor_arrows),
            ),
            (
                "base_bonds".to_string(),
                TextValue::Bool(self.include_base_bonds_in_diff),
            ),
            ("tolerance".to_string(), TextValue::Float(self.tolerance)),
            (
                "error_on_stale".to_string(),
                TextValue::Bool(self.error_on_stale_entries),
            ),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("output_diff") {
            self.output_diff = v.as_bool().ok_or("output_diff must be a bool")?;
        }
        if let Some(v) = props.get("show_anchor_arrows") {
            self.show_anchor_arrows = v.as_bool().ok_or("show_anchor_arrows must be a bool")?;
        }
        if let Some(v) = props.get("base_bonds") {
            self.include_base_bonds_in_diff = v.as_bool().ok_or("base_bonds must be a bool")?;
        }
        if let Some(v) = props.get("tolerance") {
            self.tolerance = v.as_float().ok_or("tolerance must be a number")?;
        }
        if let Some(v) = props.get("error_on_stale") {
            self.error_on_stale_entries = v.as_bool().ok_or("error_on_stale must be a bool")?;
        }
        if let Some(v) = props.get("diff") {
            let diff_text = v.as_string().ok_or("diff must be a string")?;
            if diff_text.trim().is_empty() {
                self.diff = AtomicStructure::new_diff();
            } else {
                self.diff = parse_diff_text(diff_text)?;
            }
            // Clear selection since the diff has been replaced
            self.selection.clear();
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("molecule".to_string(), (false, None)); // optional: allows creating from scratch
        m
    }
}

// =============================================================================
// Node type registration
// =============================================================================

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "atom_edit".to_string(),
        description: "Diff-based atomic structure editing. Represents edits as a diff \
            (additions, deletions, replacements, moves) applied to the input structure. \
            Selection is transient and not serialized.\n\
            \n\
            The 'diff' property uses a line-based text format:\n\
            \n\
            Atom lines:\n\
            +C @ (1.0, 2.0, 3.0)                         # Add carbon atom at position\n\
            - @ (4.0, 5.0, 6.0)                           # Delete atom at position\n\
            ~C @ (7.0, 8.0, 9.0)                          # Replace atom at position (e.g. Si->C)\n\
            ~Si @ (7.0, 8.0, 9.0) [from (7.0, 8.5, 9.0)] # Move atom: Si at new pos, matched at old pos\n\
            \n\
            The ~ prefix means the atom is expected to match a base atom (replacement or move).\n\
            The + prefix means a new atom addition. Both use positional matching internally.\n\
            \n\
            Bond lines (atom indices are 1-based, referencing atom line order above):\n\
            bond 1-2 single                                # Add bond (single/double/triple/aromatic/...)\n\
            unbond 3-4                                     # Delete bond between atoms 3 and 4\n\
            \n\
            Supported bond orders: single, double, triple, quadruple, aromatic, dative, metallic.\n\
            Lines starting with # are comments. Blank lines are ignored."
            .to_string(),
        summary: Some("Edit atoms via diff".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![Parameter {
            id: None,
            name: "molecule".to_string(),
            data_type: DataType::Atomic,
        }],
        output_type: DataType::Atomic,
        public: true,
        node_data_creator: || Box::new(AtomEditData::new()),
        node_data_saver: |node_data, _design_dir| {
            if let Some(data) = node_data.as_any_mut().downcast_ref::<AtomEditData>() {
                let serializable = atom_edit_data_to_serializable(data)?;
                serde_json::to_value(serializable)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Data type mismatch for atom_edit",
                ))
            }
        },
        node_data_loader: |value, _design_dir| {
            let serializable: SerializableAtomEditData = serde_json::from_value(value.clone())
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            Ok(Box::new(serializable_to_atom_edit_data(&serializable)?))
        },
    }
}

// =============================================================================
// Helper accessors
// =============================================================================

/// Gets the AtomEditData for the currently active atom_edit node (immutable)
pub fn get_active_atom_edit_data(structure_designer: &StructureDesigner) -> Option<&AtomEditData> {
    let selected_node_id = structure_designer.get_selected_node_id_with_type("atom_edit")?;
    let node_data = structure_designer.get_node_network_data(selected_node_id)?;
    node_data.as_any_ref().downcast_ref::<AtomEditData>()
}

/// Gets mutable access to AtomEditData WITHOUT marking the node data as changed.
/// Use for transient state changes (interaction_state) that don't affect evaluation.
pub(super) fn get_atom_edit_data_mut_transient(
    structure_designer: &mut StructureDesigner,
) -> Option<&mut AtomEditData> {
    let selected_node_id = structure_designer.get_selected_node_id_with_type("atom_edit")?;
    let node_data = structure_designer.get_node_network_data_mut(selected_node_id)?;
    node_data.as_any_mut().downcast_mut::<AtomEditData>()
}

/// Gets the AtomEditData for the currently selected atom_edit node (mutable)
///
/// Automatically marks the node data as changed since this is only called for mutations.
pub fn get_selected_atom_edit_data_mut(
    structure_designer: &mut StructureDesigner,
) -> Option<&mut AtomEditData> {
    let selected_node_id = structure_designer.get_selected_node_id_with_type("atom_edit")?;
    structure_designer.mark_node_data_changed(selected_node_id);
    let node_data = structure_designer.get_node_network_data_mut(selected_node_id)?;
    node_data.as_any_mut().downcast_mut::<AtomEditData>()
}

// =============================================================================
// Utility functions (shared across submodules)
// =============================================================================

/// Calculate a selection transform from a list of atom positions.
pub(super) fn calc_transform_from_positions(positions: &[DVec3]) -> Option<Transform> {
    if positions.is_empty() {
        return None;
    }

    let avg_position =
        positions.iter().fold(DVec3::ZERO, |acc, pos| acc + *pos) / positions.len() as f64;

    let mut transform = Transform {
        translation: avg_position,
        ..Default::default()
    };

    if positions.len() >= 4 || positions.len() == 1 {
        transform.rotation = DQuat::IDENTITY;
    } else if positions.len() >= 2 {
        let x_axis = (positions[1] - positions[0]).normalize();
        let local_x_axis = DVec3::new(1.0, 0.0, 0.0);
        transform.rotation = DQuat::from_rotation_arc(local_x_axis, x_axis);

        if positions.len() == 3 {
            let global_x_axis = transform.rotation.mul_vec3(local_x_axis);
            let atom1_to_atom3 = positions[2] - positions[0];
            let projection = atom1_to_atom3.dot(global_x_axis) * global_x_axis;
            let perpendicular = atom1_to_atom3 - projection;

            if perpendicular.length_squared() > 0.00001 {
                let new_z_axis = perpendicular.normalize();
                let global_z_axis = transform.rotation.mul_vec3(DVec3::new(0.0, 0.0, 1.0));
                let angle = global_z_axis.angle_between(new_z_axis);
                let cross = global_z_axis.cross(new_z_axis);
                let sign = if cross.dot(global_x_axis) < 0.0 {
                    -1.0
                } else {
                    1.0
                };
                let x_rotation = DQuat::from_axis_angle(global_x_axis, sign * angle);
                transform.rotation = x_rotation * transform.rotation;
            }
        }
    }

    Some(transform)
}

/// Calculate selection transform from provenance-based selection.
#[allow(dead_code)]
pub(crate) fn calc_atom_edit_selection_transform(
    result_structure: &AtomicStructure,
    selection: &AtomEditSelection,
    provenance: &DiffProvenance,
) -> Option<Transform> {
    let mut positions: Vec<DVec3> = Vec::new();

    for &base_id in &selection.selected_base_atoms {
        if let Some(&result_id) = provenance.base_to_result.get(&base_id) {
            if let Some(atom) = result_structure.get_atom(result_id) {
                positions.push(atom.position);
            }
        }
    }

    for &diff_id in &selection.selected_diff_atoms {
        if let Some(&result_id) = provenance.diff_to_result.get(&diff_id) {
            if let Some(atom) = result_structure.get_atom(result_id) {
                positions.push(atom.position);
            }
        }
    }

    calc_transform_from_positions(&positions)
}

/// Apply a select modifier to a HashSet of atom IDs.
pub(super) fn apply_modifier_to_set(set: &mut HashSet<u32>, id: u32, modifier: &SelectModifier) {
    match modifier {
        SelectModifier::Replace | SelectModifier::Expand => {
            set.insert(id);
        }
        SelectModifier::Toggle => {
            if !set.remove(&id) {
                set.insert(id);
            }
        }
    }
}

/// Extract the diff atom ID from an AtomSource, if present.
pub(super) fn get_diff_id_from_source(source: &AtomSource) -> Option<u32> {
    match source {
        AtomSource::BasePassthrough(_) => None,
        AtomSource::DiffMatchedBase { diff_id, .. } => Some(*diff_id),
        AtomSource::DiffAdded(diff_id) => Some(*diff_id),
    }
}
