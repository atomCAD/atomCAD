use super::text_format::{parse_diff_text, serialize_diff};
use crate::api::common_api_types::SelectModifier;
use crate::api::structure_designer::structure_designer_api_types::APIAtomEditTool;
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::api::structure_designer::structure_designer_preferences::AtomicStructureVisualization;
use crate::crystolecule::atomic_structure::HitTestResult;
use crate::crystolecule::atomic_structure::{AtomicStructure, BondReference};
use crate::crystolecule::atomic_structure_diff::{
    AtomSource, DiffProvenance, DiffStats, apply_diff, enrich_diff_with_base_bonds,
};
use crate::crystolecule::simulation::minimize::MinimizationConfig;
use crate::crystolecule::simulation::minimize::minimize_with_force_field;
use crate::crystolecule::simulation::topology::MolecularTopology;
use crate::crystolecule::simulation::uff::{UffForceField, VdwMode};
use crate::display::atomic_tessellator::{BAS_STICK_RADIUS, get_displayed_atom_radius};
use crate::display::preferences as display_prefs;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{NodeType, Parameter};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use crate::util::transform::Transform;
use glam::f64::{DQuat, DVec3};
use std::collections::{HashMap, HashSet};
use std::io;

use crate::structure_designer::serialization::atom_edit_data_serialization::{
    SerializableAtomEditData, atom_edit_data_to_serializable, serializable_to_atom_edit_data,
};

/// Default positional matching tolerance in Angstroms.
pub const DEFAULT_TOLERANCE: f64 = 0.1;

// --- Tool state structs ---

#[derive(Debug)]
pub struct DefaultToolState {
    pub replacement_atomic_number: i16,
}

#[derive(Debug)]
pub struct AddAtomToolState {
    pub atomic_number: i16,
}

#[derive(Debug)]
pub struct AddBondToolState {
    pub last_atom_id: Option<u32>,
}

#[derive(Debug)]
pub enum AtomEditTool {
    Default(DefaultToolState),
    AddAtom(AddAtomToolState),
    AddBond(AddBondToolState),
}

// --- Selection model ---

/// Provenance-based selection state for atom_edit.
///
/// Selection is stored by provenance (base/diff atom IDs) rather than result atom IDs.
/// This makes selection stable across re-evaluations, since base IDs are immutable
/// and diff IDs are under our control.
#[derive(Debug, Clone, Default)]
pub struct AtomEditSelection {
    /// Base atoms selected (by base atom ID — stable, input doesn't change during editing)
    pub selected_base_atoms: HashSet<u32>,
    /// Diff atoms selected (by diff atom ID — stable, we control the diff)
    pub selected_diff_atoms: HashSet<u32>,
    /// Bond selection in result space (cleared on any diff mutation)
    pub selected_bonds: HashSet<BondReference>,
    /// Cached selection transform (recalculated after selection changes)
    pub selection_transform: Option<Transform>,
}

impl AtomEditSelection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.selected_base_atoms.is_empty()
            && self.selected_diff_atoms.is_empty()
            && self.selected_bonds.is_empty()
    }

    pub fn has_selected_atoms(&self) -> bool {
        !self.selected_base_atoms.is_empty() || !self.selected_diff_atoms.is_empty()
    }

    pub fn clear(&mut self) {
        self.selected_base_atoms.clear();
        self.selected_diff_atoms.clear();
        self.selected_bonds.clear();
        self.selection_transform = None;
    }

    /// Clear bond selection (called when diff is mutated)
    pub fn clear_bonds(&mut self) {
        self.selected_bonds.clear();
    }
}

// --- Eval cache ---

/// Evaluation cache for the atom_edit node.
///
/// Stores the provenance and stats computed during the most recent `apply_diff()` call.
/// Retrieved by interaction functions via `structure_designer.get_selected_node_eval_cache()`.
#[derive(Debug, Clone)]
pub struct AtomEditEvalCache {
    pub provenance: DiffProvenance,
    pub stats: DiffStats,
}

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

    // Transient (NOT serialized)
    /// Current selection state
    pub selection: AtomEditSelection,
    /// Current editing tool
    pub active_tool: AtomEditTool,
    /// Last known diff stats (updated during eval, used by get_subtitle)
    last_stats: Option<DiffStats>,
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
            selection: AtomEditSelection::new(),
            active_tool: AtomEditTool::Default(DefaultToolState {
                replacement_atomic_number: 6, // Default to carbon
            }),
            last_stats: None,
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
    ) -> Self {
        Self {
            diff,
            output_diff,
            show_anchor_arrows,
            include_base_bonds_in_diff,
            tolerance,
            selection: AtomEditSelection::new(),
            active_tool: AtomEditTool::Default(DefaultToolState {
                replacement_atomic_number: 6,
            }),
            last_stats: None,
        }
    }

    // --- Direct diff mutation methods ---
    // These are called by interaction functions (Phase 4).
    // Included here as the core diff manipulation API.

    /// Add an atom to the diff at the given position.
    /// Returns the new atom's ID in the diff.
    pub fn add_atom_to_diff(&mut self, atomic_number: i16, position: glam::f64::DVec3) -> u32 {
        self.selection.clear_bonds();
        self.diff.add_atom(atomic_number, position)
    }

    /// Add a delete marker at the given position.
    /// Returns the delete marker's ID in the diff.
    pub fn mark_for_deletion(&mut self, match_position: glam::f64::DVec3) -> u32 {
        self.selection.clear_bonds();
        self.diff.add_atom(
            crate::crystolecule::atomic_structure::DELETED_SITE_ATOMIC_NUMBER,
            match_position,
        )
    }

    /// Add/update an atom in the diff with a new atomic number at the given position.
    /// Returns the atom's ID in the diff.
    pub fn replace_in_diff(
        &mut self,
        match_position: glam::f64::DVec3,
        new_atomic_number: i16,
    ) -> u32 {
        self.selection.clear_bonds();
        self.diff.add_atom(new_atomic_number, match_position)
    }

    /// Move an atom in the diff to a new position.
    /// Sets anchor to the current position if not already set (first move).
    pub fn move_in_diff(&mut self, atom_id: u32, new_position: glam::f64::DVec3) {
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
        self.diff.add_bond(atom_id1, atom_id2, order);
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
        self.active_tool = match api_tool {
            APIAtomEditTool::Default => AtomEditTool::Default(DefaultToolState {
                replacement_atomic_number: 6,
            }),
            APIAtomEditTool::AddAtom => {
                AtomEditTool::AddAtom(AddAtomToolState { atomic_number: 6 })
            }
            APIAtomEditTool::AddBond => {
                AtomEditTool::AddBond(AddBondToolState { last_atom_id: None })
            }
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
                state.atomic_number = atomic_number;
                true
            }
            _ => false,
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
    ///
    /// - `base_atoms`: (base_id, position) — adds delete markers at these positions
    /// - `diff_atoms`: (diff_id, is_pure_addition) — removes pure additions,
    ///   converts matched atoms to delete markers
    /// - `bonds`: bond deletion info for adding bond delete markers
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

    /// Apply deletion in diff view (reversal semantics). Called by
    /// `delete_selected_in_diff_view` after gathering selected IDs.
    ///
    /// - `diff_atoms`: (diff_id, DiffAtomKind) — action depends on kind
    /// - `bonds`: bond references to remove from diff
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
    ///
    /// - `atomic_number`: the new element
    /// - `base_atoms`: (base_id, position) — adds to diff with new element
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
    ///
    /// - `relative`: the delta transform to apply
    /// - `base_atoms`: (base_id, atomic_number, old_position) — adds to diff with anchor
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
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        network_evaluator: &NetworkEvaluator,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        decorate: bool,
        context: &mut crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext,
    ) -> NetworkResult {
        let input_val =
            network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);

        if let NetworkResult::Error(_) = input_val {
            return input_val;
        }

        if let NetworkResult::Atomic(input_structure) = input_val {
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
                }
                return NetworkResult::Atomic(diff_clone);
            }

            // Apply the diff to the input
            let diff_result = apply_diff(&input_structure, &self.diff, self.tolerance);

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

                // Mark AddBond tool's last atom if applicable
                if let AtomEditTool::AddBond(state) = &self.active_tool {
                    if let Some(last_diff_id) = state.last_atom_id {
                        // Map diff atom ID to result atom ID
                        if let Some(&result_id) =
                            diff_result.provenance.diff_to_result.get(&last_diff_id)
                        {
                            result.decorator_mut().set_atom_display_state(
                                result_id,
                                crate::crystolecule::atomic_structure::AtomDisplayState::Marked,
                            );
                        }
                    }
                }
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
        } else {
            NetworkResult::Atomic(AtomicStructure::new())
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(AtomEditData {
            diff: self.diff.clone(),
            output_diff: self.output_diff,
            show_anchor_arrows: self.show_anchor_arrows,
            include_base_bonds_in_diff: self.include_base_bonds_in_diff,
            tolerance: self.tolerance,
            selection: self.selection.clone(),
            active_tool: match &self.active_tool {
                AtomEditTool::Default(state) => AtomEditTool::Default(DefaultToolState {
                    replacement_atomic_number: state.replacement_atomic_number,
                }),
                AtomEditTool::AddAtom(state) => AtomEditTool::AddAtom(AddAtomToolState {
                    atomic_number: state.atomic_number,
                }),
                AtomEditTool::AddBond(state) => AtomEditTool::AddBond(AddBondToolState {
                    last_atom_id: state.last_atom_id,
                }),
            },
            last_stats: self.last_stats.clone(),
        })
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
        m.insert("molecule".to_string(), (true, None)); // required
        m
    }
}

// =============================================================================
// Minimization
// =============================================================================

/// Freeze mode for atom_edit energy minimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinimizeFreezeMode {
    /// Only diff atoms move; base atoms are frozen at their original positions.
    FreezeBase,
    /// All atoms move freely.
    FreeAll,
    /// Only selected atoms move; everything else is frozen.
    FreeSelected,
}

/// Minimizes the atomic structure in the active atom_edit node using UFF.
///
/// Evaluates the full base+diff structure, runs L-BFGS minimization with the
/// chosen freeze strategy, and writes moved atom positions back into the diff.
///
/// Returns a human-readable result message, or an error string.
pub fn minimize_atom_edit(
    structure_designer: &mut StructureDesigner,
    freeze_mode: MinimizeFreezeMode,
) -> Result<String, String> {
    // Phase 1: Gather info (immutable borrows, all owned data returned)
    let (topology, force_field, frozen_indices, result_to_source) = {
        let atom_edit_data =
            get_active_atom_edit_data(structure_designer).ok_or("No active atom_edit node")?;

        // Check if we're in diff view — minimization always operates on the full result
        if atom_edit_data.output_diff {
            return Err("Switch to result view before minimizing".to_string());
        }

        let eval_cache = structure_designer
            .get_selected_node_eval_cache()
            .ok_or("No eval cache")?;
        let eval_cache = eval_cache
            .downcast_ref::<AtomEditEvalCache>()
            .ok_or("Wrong eval cache type")?;

        let result_structure = structure_designer
            .get_atomic_structure_from_selected_node()
            .ok_or("No result structure")?;

        // Build topology from the evaluated result
        let vdw_mode = if structure_designer
            .preferences
            .simulation_preferences
            .use_vdw_cutoff
        {
            VdwMode::Cutoff(6.0)
        } else {
            VdwMode::AllPairs
        };
        let topology = match &vdw_mode {
            VdwMode::AllPairs => MolecularTopology::from_structure(result_structure),
            VdwMode::Cutoff(_) => {
                MolecularTopology::from_structure_bonded_only(result_structure)
            }
        };
        if topology.num_atoms == 0 {
            return Ok("No atoms to minimize".to_string());
        }

        // Build topology_index → AtomSource map for write-back
        let result_to_source: Vec<Option<AtomSource>> = topology
            .atom_ids
            .iter()
            .map(|&result_id| eval_cache.provenance.sources.get(&result_id).cloned())
            .collect();

        // Determine frozen set (topology indices) — computed before force field
        // so cutoff mode can skip frozen-frozen vdW pairs.
        let frozen_indices: Vec<usize> = match freeze_mode {
            MinimizeFreezeMode::FreezeBase => topology
                .atom_ids
                .iter()
                .enumerate()
                .filter(|(_, result_id)| {
                    matches!(
                        eval_cache.provenance.sources.get(result_id),
                        Some(AtomSource::BasePassthrough(_))
                    )
                })
                .map(|(i, _)| i)
                .collect(),
            MinimizeFreezeMode::FreeAll => Vec::new(),
            MinimizeFreezeMode::FreeSelected => {
                // Build set of selected result atom IDs from selection + provenance
                let mut selected_result_ids: HashSet<u32> = HashSet::new();
                for &base_id in &atom_edit_data.selection.selected_base_atoms {
                    if let Some(&result_id) =
                        eval_cache.provenance.base_to_result.get(&base_id)
                    {
                        selected_result_ids.insert(result_id);
                    }
                }
                for &diff_id in &atom_edit_data.selection.selected_diff_atoms {
                    if let Some(&result_id) =
                        eval_cache.provenance.diff_to_result.get(&diff_id)
                    {
                        selected_result_ids.insert(result_id);
                    }
                }
                if selected_result_ids.is_empty() {
                    return Err(
                        "No atoms selected — select atoms to minimize first".to_string(),
                    );
                }
                // Freeze everything NOT selected
                topology
                    .atom_ids
                    .iter()
                    .enumerate()
                    .filter(|(_, result_id)| !selected_result_ids.contains(result_id))
                    .map(|(i, _)| i)
                    .collect()
            }
        };

        let force_field =
            UffForceField::from_topology_with_frozen(&topology, vdw_mode, &frozen_indices)?;

        (topology, force_field, frozen_indices, result_to_source)
    };

    // Phase 2: Minimize (no borrows on structure_designer)
    let mut positions = topology.positions.clone();
    let config = MinimizationConfig::default();
    let start = std::time::Instant::now();
    let result = minimize_with_force_field(&force_field, &mut positions, &config, &frozen_indices);
    let elapsed_ms = start.elapsed().as_millis();

    // Phase 3: Write back moved positions into the diff (mutable borrow)
    let atom_edit_data =
        get_selected_atom_edit_data_mut(structure_designer).ok_or("No active atom_edit node")?;

    for (topo_idx, source) in result_to_source.iter().enumerate() {
        let new_pos = DVec3::new(
            positions[topo_idx * 3],
            positions[topo_idx * 3 + 1],
            positions[topo_idx * 3 + 2],
        );
        let old_pos = DVec3::new(
            topology.positions[topo_idx * 3],
            topology.positions[topo_idx * 3 + 1],
            topology.positions[topo_idx * 3 + 2],
        );

        if (new_pos - old_pos).length() < 1e-6 {
            continue;
        }

        match source {
            Some(AtomSource::DiffAdded(diff_id))
            | Some(AtomSource::DiffMatchedBase { diff_id, .. }) => {
                atom_edit_data.diff.set_atom_position(*diff_id, new_pos);
            }
            Some(AtomSource::BasePassthrough(_)) => {
                // FreeAll mode only — base atom moved, add to diff with anchor
                let atomic_number = topology.atomic_numbers[topo_idx];
                let new_diff_id = atom_edit_data.diff.add_atom(atomic_number, new_pos);
                atom_edit_data
                    .diff
                    .set_anchor_position(new_diff_id, old_pos);
            }
            None => {
                // No provenance info — skip
            }
        }
    }

    Ok(format!(
        "Minimization {} after {} iterations (energy: {:.4} kcal/mol, {}ms)",
        if result.converged {
            "converged"
        } else {
            "stopped"
        },
        result.iterations,
        result.energy,
        elapsed_ms
    ))
}

// --- Helper functions for accessing AtomEditData from StructureDesigner ---

/// Gets the AtomEditData for the currently active atom_edit node (immutable)
pub fn get_active_atom_edit_data(structure_designer: &StructureDesigner) -> Option<&AtomEditData> {
    let selected_node_id = structure_designer.get_selected_node_id_with_type("atom_edit")?;
    let node_data = structure_designer.get_node_network_data(selected_node_id)?;
    node_data.as_any_ref().downcast_ref::<AtomEditData>()
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
// Phase 4: Interaction Functions
// =============================================================================

// --- Private helpers ---

/// Calculate a selection transform from a list of atom positions.
///
/// Same logic as `calc_selection_transform` in atomic_structure_utils.rs,
/// but operates on positions directly rather than reading selected flags.
fn calc_transform_from_positions(positions: &[DVec3]) -> Option<Transform> {
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
///
/// Maps selected base/diff atom IDs to positions in the result structure
/// via provenance, then computes the transform centroid and orientation.
///
/// Used by the API layer (Phase 5) for `get_atom_edit_data()`.
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
fn apply_modifier_to_set(set: &mut HashSet<u32>, id: u32, modifier: &SelectModifier) {
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

/// Info needed to delete a bond: diff atom IDs for both endpoints
/// (None if the atom needs an identity entry), plus the atom info for identity entries.
#[derive(Debug, Clone)]
pub struct BondDeletionInfo {
    pub diff_id_a: Option<u32>,
    pub diff_id_b: Option<u32>,
    pub identity_a: Option<(i16, DVec3)>,
    pub identity_b: Option<(i16, DVec3)>,
}

/// Extract the diff atom ID from an AtomSource, if present.
fn get_diff_id_from_source(source: &AtomSource) -> Option<u32> {
    match source {
        AtomSource::BasePassthrough(_) => None,
        AtomSource::DiffMatchedBase { diff_id, .. } => Some(*diff_id),
        AtomSource::DiffAdded(diff_id) => Some(*diff_id),
    }
}

// --- Public interaction functions ---

/// Select an atom or bond by ray hit test.
///
/// Returns true if something was hit, false otherwise.
pub fn select_atom_or_bond_by_ray(
    structure_designer: &mut StructureDesigner,
    ray_start: &DVec3,
    ray_dir: &DVec3,
    select_modifier: SelectModifier,
) -> bool {
    // Phase 1: Hit test (immutable borrow)
    let hit_result = {
        let result_structure = match structure_designer.get_atomic_structure_from_selected_node() {
            Some(s) => s,
            None => return false,
        };

        let visualization = &structure_designer
            .preferences
            .atomic_structure_visualization_preferences
            .visualization;
        let display_visualization = match visualization {
            AtomicStructureVisualization::BallAndStick => {
                display_prefs::AtomicStructureVisualization::BallAndStick
            }
            AtomicStructureVisualization::SpaceFilling => {
                display_prefs::AtomicStructureVisualization::SpaceFilling
            }
        };

        result_structure.hit_test(
            ray_start,
            ray_dir,
            visualization,
            |atom| get_displayed_atom_radius(atom, &display_visualization),
            BAS_STICK_RADIUS,
        )
    };

    // In diff view, atom IDs from the hit test are diff-native IDs — no provenance needed
    let is_diff_view = match get_active_atom_edit_data(structure_designer) {
        Some(data) => data.output_diff,
        None => false,
    };

    match hit_result {
        HitTestResult::Atom(atom_id, _distance) => {
            if is_diff_view {
                select_diff_atom_directly(structure_designer, atom_id, select_modifier)
            } else {
                select_result_atom(structure_designer, atom_id, select_modifier)
            }
        }
        HitTestResult::Bond(bond_reference, _distance) => {
            select_result_bond(structure_designer, &bond_reference, select_modifier)
        }
        HitTestResult::None => false,
    }
}

/// Select an atom by its result atom ID, using provenance to categorize it.
fn select_result_atom(
    structure_designer: &mut StructureDesigner,
    result_atom_id: u32,
    select_modifier: SelectModifier,
) -> bool {
    // Phase 1: Gather info (immutable borrows)
    let (atom_source, clicked_position, mut position_map) = {
        let eval_cache = match structure_designer.get_selected_node_eval_cache() {
            Some(cache) => cache,
            None => return false,
        };
        let eval_cache = match eval_cache.downcast_ref::<AtomEditEvalCache>() {
            Some(cache) => cache,
            None => return false,
        };
        let result_structure = match structure_designer.get_atomic_structure_from_selected_node() {
            Some(s) => s,
            None => return false,
        };
        let atom_edit_data = match get_active_atom_edit_data(structure_designer) {
            Some(data) => data,
            None => return false,
        };

        let atom_source = match eval_cache.provenance.sources.get(&result_atom_id) {
            Some(s) => s.clone(),
            None => return false,
        };
        let clicked_pos = match result_structure.get_atom(result_atom_id) {
            Some(a) => a.position,
            None => return false,
        };

        // Pre-collect positions for currently selected atoms (needed for transform calculation)
        let mut sel_positions: HashMap<(bool, u32), DVec3> = HashMap::new();
        for &base_id in &atom_edit_data.selection.selected_base_atoms {
            if let Some(&res_id) = eval_cache.provenance.base_to_result.get(&base_id) {
                if let Some(atom) = result_structure.get_atom(res_id) {
                    sel_positions.insert((false, base_id), atom.position);
                }
            }
        }
        for &diff_id in &atom_edit_data.selection.selected_diff_atoms {
            if let Some(&res_id) = eval_cache.provenance.diff_to_result.get(&diff_id) {
                if let Some(atom) = result_structure.get_atom(res_id) {
                    sel_positions.insert((true, diff_id), atom.position);
                }
            }
        }

        (atom_source, clicked_pos, sel_positions)
    };

    // Add clicked atom to position map (may not be there if newly selected)
    match &atom_source {
        AtomSource::BasePassthrough(base_id) => {
            position_map.insert((false, *base_id), clicked_position);
        }
        AtomSource::DiffMatchedBase { diff_id, base_id } => {
            position_map.insert((true, *diff_id), clicked_position);
            // Clean up stale base entry if present
            position_map.remove(&(false, *base_id));
        }
        AtomSource::DiffAdded(diff_id) => {
            position_map.insert((true, *diff_id), clicked_position);
        }
    }

    // Phase 2: Mutate selection
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return false,
    };

    // Handle Replace modifier (clear all first)
    if matches!(select_modifier, SelectModifier::Replace) {
        atom_edit_data.selection.clear();
    }

    // Add/toggle in appropriate selection set based on provenance
    match &atom_source {
        AtomSource::BasePassthrough(base_id) => {
            apply_modifier_to_set(
                &mut atom_edit_data.selection.selected_base_atoms,
                *base_id,
                &select_modifier,
            );
        }
        AtomSource::DiffMatchedBase { diff_id, base_id } => {
            // Clean up: remove from base selection if present (atom is now in diff)
            atom_edit_data.selection.selected_base_atoms.remove(base_id);
            apply_modifier_to_set(
                &mut atom_edit_data.selection.selected_diff_atoms,
                *diff_id,
                &select_modifier,
            );
        }
        AtomSource::DiffAdded(diff_id) => {
            apply_modifier_to_set(
                &mut atom_edit_data.selection.selected_diff_atoms,
                *diff_id,
                &select_modifier,
            );
        }
    }

    // Recalculate selection transform from positions
    let positions: Vec<DVec3> = atom_edit_data
        .selection
        .selected_base_atoms
        .iter()
        .filter_map(|&id| position_map.get(&(false, id)).copied())
        .chain(
            atom_edit_data
                .selection
                .selected_diff_atoms
                .iter()
                .filter_map(|&id| position_map.get(&(true, id)).copied()),
        )
        .collect();

    atom_edit_data.selection.selection_transform = calc_transform_from_positions(&positions);

    true
}

/// Select an atom directly in diff view (no provenance needed).
///
/// In diff view, the displayed structure IS the diff, so atom IDs from the hit test
/// are diff atom IDs. All selected atoms go into `selected_diff_atoms`.
fn select_diff_atom_directly(
    structure_designer: &mut StructureDesigner,
    diff_atom_id: u32,
    select_modifier: SelectModifier,
) -> bool {
    // Phase 1: Gather positions (immutable borrow)
    let (clicked_position, mut position_map) = {
        let displayed_structure = match structure_designer.get_atomic_structure_from_selected_node()
        {
            Some(s) => s,
            None => return false,
        };
        let atom_edit_data = match get_active_atom_edit_data(structure_designer) {
            Some(data) => data,
            None => return false,
        };

        let clicked_pos = match displayed_structure.get_atom(diff_atom_id) {
            Some(a) => a.position,
            None => return false,
        };

        // Collect positions for currently selected diff atoms
        let mut sel_positions: HashMap<u32, DVec3> = HashMap::new();
        for &id in &atom_edit_data.selection.selected_diff_atoms {
            if let Some(atom) = displayed_structure.get_atom(id) {
                sel_positions.insert(id, atom.position);
            }
        }

        (clicked_pos, sel_positions)
    };

    position_map.insert(diff_atom_id, clicked_position);

    // Phase 2: Mutate selection
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return false,
    };

    if matches!(select_modifier, SelectModifier::Replace) {
        atom_edit_data.selection.clear();
    }

    apply_modifier_to_set(
        &mut atom_edit_data.selection.selected_diff_atoms,
        diff_atom_id,
        &select_modifier,
    );

    // Recalculate selection transform from diff atom positions
    let positions: Vec<DVec3> = atom_edit_data
        .selection
        .selected_diff_atoms
        .iter()
        .filter_map(|&id| position_map.get(&id).copied())
        .collect();

    atom_edit_data.selection.selection_transform = calc_transform_from_positions(&positions);

    true
}

/// Select a bond by its reference in result space.
fn select_result_bond(
    structure_designer: &mut StructureDesigner,
    bond_reference: &BondReference,
    select_modifier: SelectModifier,
) -> bool {
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return false,
    };

    if matches!(select_modifier, SelectModifier::Replace) {
        atom_edit_data.selection.clear();
    }

    match select_modifier {
        SelectModifier::Replace | SelectModifier::Expand => {
            atom_edit_data
                .selection
                .selected_bonds
                .insert(bond_reference.clone());
        }
        SelectModifier::Toggle => {
            if !atom_edit_data
                .selection
                .selected_bonds
                .remove(bond_reference)
            {
                atom_edit_data
                    .selection
                    .selected_bonds
                    .insert(bond_reference.clone());
            }
        }
    }

    true
}

/// Add an atom at the ray-plane intersection point.
///
/// The plane passes through the closest atom to the ray (or at a default distance).
pub fn add_atom_by_ray(
    structure_designer: &mut StructureDesigner,
    atomic_number: i16,
    plane_normal: &DVec3,
    ray_start: &DVec3,
    ray_dir: &DVec3,
) {
    // Phase 1: Calculate position (immutable borrow)
    let position = {
        let atomic_structure = match structure_designer.get_atomic_structure_from_selected_node() {
            Some(structure) => structure,
            None => return,
        };

        let closest_atom_position = atomic_structure.find_closest_atom_to_ray(ray_start, ray_dir);
        let default_distance = 5.0;
        let plane_distance = match closest_atom_position {
            Some(atom_pos) => plane_normal.dot(atom_pos),
            None => plane_normal.dot(*ray_start) + default_distance,
        };

        let denominator = plane_normal.dot(*ray_dir);
        if denominator.abs() < 1e-6 {
            return;
        }

        let t = (plane_distance - plane_normal.dot(*ray_start)) / denominator;
        if t < 0.0 {
            return;
        }

        *ray_start + *ray_dir * t
    };

    // Phase 2: Add atom to diff
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return,
    };

    atom_edit_data.add_atom_to_diff(atomic_number, position);
}

/// Draw a bond by clicking on atoms (two-click workflow).
///
/// First click stores the atom, second click creates the bond.
/// Clicking the same atom again cancels the pending bond.
pub fn draw_bond_by_ray(
    structure_designer: &mut StructureDesigner,
    ray_start: &DVec3,
    ray_dir: &DVec3,
) {
    // Phase 1: Hit test and gather info (immutable borrows)
    let is_diff_view = match get_active_atom_edit_data(structure_designer) {
        Some(data) => data.output_diff,
        None => return,
    };

    let (atom_source, atom_info) = {
        let result_structure = match structure_designer.get_atomic_structure_from_selected_node() {
            Some(s) => s,
            None => return,
        };

        let visualization = &structure_designer
            .preferences
            .atomic_structure_visualization_preferences
            .visualization;
        let display_visualization = match visualization {
            AtomicStructureVisualization::BallAndStick => {
                display_prefs::AtomicStructureVisualization::BallAndStick
            }
            AtomicStructureVisualization::SpaceFilling => {
                display_prefs::AtomicStructureVisualization::SpaceFilling
            }
        };

        let result_atom_id = match result_structure.hit_test(
            ray_start,
            ray_dir,
            visualization,
            |atom| get_displayed_atom_radius(atom, &display_visualization),
            BAS_STICK_RADIUS,
        ) {
            HitTestResult::Atom(id, _) => id,
            _ => return,
        };

        if is_diff_view {
            // In diff view, atom IDs are diff-native — no provenance needed
            let atom = match result_structure.get_atom(result_atom_id) {
                Some(a) => (a.atomic_number, a.position),
                None => return,
            };
            (None, (result_atom_id, atom))
        } else {
            let eval_cache = match structure_designer.get_selected_node_eval_cache() {
                Some(cache) => cache,
                None => return,
            };
            let eval_cache = match eval_cache.downcast_ref::<AtomEditEvalCache>() {
                Some(cache) => cache,
                None => return,
            };

            let source = match eval_cache.provenance.sources.get(&result_atom_id) {
                Some(s) => s.clone(),
                None => return,
            };

            let atom = match result_structure.get_atom(result_atom_id) {
                Some(a) => (a.atomic_number, a.position),
                None => return,
            };

            (Some(source), (result_atom_id, atom))
        }
    };

    // Phase 2: Resolve to diff atom ID and handle bond workflow
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return,
    };

    // Resolve to diff atom ID
    let diff_atom_id = if is_diff_view {
        // In diff view, the hit ID is already a diff atom ID
        atom_info.0
    } else {
        // In result view, map through provenance (add identity entry for base atoms)
        match &atom_source {
            Some(AtomSource::BasePassthrough(_)) => {
                atom_edit_data.diff.add_atom(atom_info.1.0, atom_info.1.1)
            }
            Some(AtomSource::DiffMatchedBase { diff_id, .. })
            | Some(AtomSource::DiffAdded(diff_id)) => *diff_id,
            None => return,
        }
    };

    // Get current last_atom_id (copies the value, ending the immutable borrow)
    let last_atom_id = if let AtomEditTool::AddBond(state) = &atom_edit_data.active_tool {
        state.last_atom_id
    } else {
        return;
    };

    match last_atom_id {
        Some(last_id) => {
            if last_id == diff_atom_id {
                // Same atom clicked again → cancel pending bond
                if let AtomEditTool::AddBond(state) = &mut atom_edit_data.active_tool {
                    state.last_atom_id = None;
                }
            } else {
                // Create bond between last atom and current atom
                atom_edit_data.add_bond_in_diff(last_id, diff_atom_id, 1);
                // Update last_atom_id for continuous bonding
                if let AtomEditTool::AddBond(state) = &mut atom_edit_data.active_tool {
                    state.last_atom_id = Some(diff_atom_id);
                }
            }
        }
        None => {
            // First click: store this atom
            if let AtomEditTool::AddBond(state) = &mut atom_edit_data.active_tool {
                state.last_atom_id = Some(diff_atom_id);
            }
        }
    }
}

/// Delete all selected atoms and bonds.
///
/// In result view:
/// - Base atoms: adds delete markers at their positions.
/// - Diff-added atoms: removed from diff entirely.
/// - Diff-matched atoms: converted to delete markers.
/// - Selected bonds: adds bond delete markers (bond_order = 0).
///
/// In diff view (reversal semantics — "delete the edit"):
/// - Delete marker atoms: removed from diff (restores base atom).
/// - Atoms with anchors (moved/replaced base atoms): converted to delete markers.
/// - Pure addition atoms: removed from diff entirely.
/// - Bond delete markers: removed from diff (restores base bond).
/// - Normal bonds: removed from diff.
pub fn delete_selected_atoms_and_bonds(structure_designer: &mut StructureDesigner) {
    let is_diff_view = match get_active_atom_edit_data(structure_designer) {
        Some(data) => data.output_diff,
        None => return,
    };

    if is_diff_view {
        delete_selected_in_diff_view(structure_designer);
    } else {
        delete_selected_in_result_view(structure_designer);
    }
}

/// Delete selected items in result view (provenance-based).
fn delete_selected_in_result_view(structure_designer: &mut StructureDesigner) {
    // Phase 1: Gather info about what to delete (immutable borrows)
    let (base_atoms_to_delete, diff_atoms_to_delete, bonds_to_delete) = {
        let eval_cache = match structure_designer.get_selected_node_eval_cache() {
            Some(cache) => cache,
            None => return,
        };
        let eval_cache = match eval_cache.downcast_ref::<AtomEditEvalCache>() {
            Some(cache) => cache,
            None => return,
        };
        let result_structure = match structure_designer.get_atomic_structure_from_selected_node() {
            Some(s) => s,
            None => return,
        };
        let atom_edit_data = match get_active_atom_edit_data(structure_designer) {
            Some(data) => data,
            None => return,
        };

        // Base atoms: need their positions for delete markers
        let mut base_to_delete: Vec<(u32, DVec3)> = Vec::new();
        for &base_id in &atom_edit_data.selection.selected_base_atoms {
            if let Some(&result_id) = eval_cache.provenance.base_to_result.get(&base_id) {
                if let Some(atom) = result_structure.get_atom(result_id) {
                    base_to_delete.push((base_id, atom.position));
                }
            }
        }

        // Diff atoms: need to know if they're pure additions or matched base atoms
        let mut diff_to_delete: Vec<(u32, bool)> = Vec::new(); // (diff_id, is_pure_addition)
        for &diff_id in &atom_edit_data.selection.selected_diff_atoms {
            let is_pure_addition = match eval_cache.provenance.diff_to_result.get(&diff_id) {
                Some(&res_id) => matches!(
                    eval_cache.provenance.sources.get(&res_id),
                    Some(AtomSource::DiffAdded(_))
                ),
                None => true, // Not in result (e.g., already a delete marker) — removable
            };
            diff_to_delete.push((diff_id, is_pure_addition));
        }

        // Bonds: need endpoint provenance and positions for identity entries
        let mut bond_deletions: Vec<BondDeletionInfo> = Vec::new();
        for bond_ref in &atom_edit_data.selection.selected_bonds {
            let source_a = eval_cache.provenance.sources.get(&bond_ref.atom_id1);
            let source_b = eval_cache.provenance.sources.get(&bond_ref.atom_id2);

            if let (Some(source_a), Some(source_b)) = (source_a, source_b) {
                let diff_id_a = get_diff_id_from_source(source_a);
                let diff_id_b = get_diff_id_from_source(source_b);

                let identity_a = if diff_id_a.is_none() {
                    result_structure
                        .get_atom(bond_ref.atom_id1)
                        .map(|a| (a.atomic_number, a.position))
                } else {
                    None
                };
                let identity_b = if diff_id_b.is_none() {
                    result_structure
                        .get_atom(bond_ref.atom_id2)
                        .map(|a| (a.atomic_number, a.position))
                } else {
                    None
                };

                bond_deletions.push(BondDeletionInfo {
                    diff_id_a,
                    diff_id_b,
                    identity_a,
                    identity_b,
                });
            }
        }

        (base_to_delete, diff_to_delete, bond_deletions)
    };

    // Phase 2: Apply deletions
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return,
    };

    atom_edit_data.apply_delete_result_view(
        &base_atoms_to_delete,
        &diff_atoms_to_delete,
        &bonds_to_delete,
    );
}

/// Delete selected items in diff view (reversal semantics).
///
/// In diff view, "delete" means "remove this edit from the diff":
/// - Delete markers → removed (restores the base atom)
/// - Moved/replaced atoms (have anchor) → converted to delete markers
/// - Pure additions → removed entirely
/// - Bond delete markers → removed (restores the base bond)
/// - Normal diff bonds → removed
fn delete_selected_in_diff_view(structure_designer: &mut StructureDesigner) {
    // Phase 1: Gather what to delete (immutable borrows)
    let (diff_atoms_to_delete, bonds_to_delete) = {
        let atom_edit_data = match get_active_atom_edit_data(structure_designer) {
            Some(data) => data,
            None => return,
        };

        let diff_atoms: Vec<(u32, DiffAtomKind)> = atom_edit_data
            .selection
            .selected_diff_atoms
            .iter()
            .map(|&diff_id| {
                let kind = classify_diff_atom(&atom_edit_data.diff, diff_id);
                (diff_id, kind)
            })
            .collect();

        let bonds: Vec<BondReference> = atom_edit_data
            .selection
            .selected_bonds
            .iter()
            .cloned()
            .collect();

        (diff_atoms, bonds)
    };

    // Phase 2: Apply deletions
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return,
    };

    atom_edit_data.apply_delete_diff_view(&diff_atoms_to_delete, &bonds_to_delete);
}

/// Classification of a diff atom based on its properties (no provenance needed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffAtomKind {
    /// Atom with atomic_number == 0 (marks a base atom for deletion)
    DeleteMarker,
    /// Atom with an anchor position (moved or replaced base atom)
    MatchedBase,
    /// Normal atom without anchor (pure addition to the structure)
    PureAddition,
}

/// Classify a diff atom by inspecting the diff structure directly.
pub fn classify_diff_atom(diff: &AtomicStructure, diff_id: u32) -> DiffAtomKind {
    if let Some(atom) = diff.get_atom(diff_id) {
        if atom.is_delete_marker() {
            DiffAtomKind::DeleteMarker
        } else if diff.has_anchor_position(diff_id) {
            DiffAtomKind::MatchedBase
        } else {
            DiffAtomKind::PureAddition
        }
    } else {
        // Atom not found — treat as removable
        DiffAtomKind::PureAddition
    }
}

/// Replace all selected atoms with a new element.
///
/// - Diff atoms: updates atomic_number in the diff directly.
/// - Base atoms: adds to diff with the new element at the base position.
///   Moves selection from selected_base_atoms to selected_diff_atoms.
pub fn replace_selected_atoms(structure_designer: &mut StructureDesigner, atomic_number: i16) {
    // Phase 1: Gather base atom info (immutable borrows)
    let base_atoms_to_replace = {
        let atom_edit_data = match get_active_atom_edit_data(structure_designer) {
            Some(data) => data,
            None => return,
        };

        // In diff view, there are no base atoms in the selection — skip provenance
        if atom_edit_data.output_diff {
            Vec::new()
        } else {
            let eval_cache = match structure_designer.get_selected_node_eval_cache() {
                Some(cache) => cache,
                None => return,
            };
            let eval_cache = match eval_cache.downcast_ref::<AtomEditEvalCache>() {
                Some(cache) => cache,
                None => return,
            };
            let result_structure =
                match structure_designer.get_atomic_structure_from_selected_node() {
                    Some(s) => s,
                    None => return,
                };

            let mut base_atoms: Vec<(u32, DVec3)> = Vec::new();
            for &base_id in &atom_edit_data.selection.selected_base_atoms {
                if let Some(&result_id) = eval_cache.provenance.base_to_result.get(&base_id) {
                    if let Some(atom) = result_structure.get_atom(result_id) {
                        base_atoms.push((base_id, atom.position));
                    }
                }
            }
            base_atoms
        }
    };

    // Phase 2: Apply replacements
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return,
    };

    atom_edit_data.apply_replace(atomic_number, &base_atoms_to_replace);
}

/// Transform selected atoms using an absolute transform.
///
/// Computes the relative delta from the current selection transform, then:
/// - Diff atoms: updates position in the diff (anchor stays).
/// - Base atoms: adds to diff at new position with anchor at old position.
///   Moves selection from selected_base_atoms to selected_diff_atoms.
///
/// Updates selection_transform algebraically (no re-evaluation needed).
pub fn transform_selected(structure_designer: &mut StructureDesigner, abs_transform: &Transform) {
    // Phase 1: Gather info (immutable borrows)
    let (current_transform, base_atoms_info) = {
        let atom_edit_data = match get_active_atom_edit_data(structure_designer) {
            Some(data) => data,
            None => return,
        };

        let current_transform = match atom_edit_data.selection.selection_transform.clone() {
            Some(t) => t,
            None => return,
        };

        // In diff view, there are no base atoms in the selection — skip provenance
        let base_info: Vec<(u32, i16, DVec3)> = if atom_edit_data.output_diff {
            Vec::new()
        } else {
            let eval_cache = match structure_designer.get_selected_node_eval_cache() {
                Some(cache) => cache,
                None => return,
            };
            let eval_cache = match eval_cache.downcast_ref::<AtomEditEvalCache>() {
                Some(cache) => cache,
                None => return,
            };
            let result_structure =
                match structure_designer.get_atomic_structure_from_selected_node() {
                    Some(s) => s,
                    None => return,
                };

            // Collect base atom info for adding to diff with anchors
            let mut info: Vec<(u32, i16, DVec3)> = Vec::new();
            for &base_id in &atom_edit_data.selection.selected_base_atoms {
                if let Some(&result_id) = eval_cache.provenance.base_to_result.get(&base_id) {
                    if let Some(atom) = result_structure.get_atom(result_id) {
                        info.push((base_id, atom.atomic_number, atom.position));
                    }
                }
            }
            info
        };

        (current_transform, base_info)
    };

    // Compute relative transform (delta from current to desired)
    let relative = abs_transform.delta_from(&current_transform);

    // Phase 2: Apply transforms
    let atom_edit_data = match get_selected_atom_edit_data_mut(structure_designer) {
        Some(data) => data,
        None => return,
    };

    atom_edit_data.apply_transform(&relative, &base_atoms_info);
}
