use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::crystolecule::atomic_structure::BondReference;
use crate::crystolecule::atomic_structure_diff::{DiffProvenance, DiffStats, apply_diff};
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{NodeType, Parameter};
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::util::transform::Transform;
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
        tolerance: f64,
    ) -> Self {
        Self {
            diff,
            output_diff,
            show_anchor_arrows,
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
                diff_clone.decorator_mut().show_anchor_arrows = self.show_anchor_arrows;
                if decorate {
                    diff_clone.decorator_mut().from_selected_node = true;
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

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("molecule".to_string(), (true, None)); // required
        m
    }
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
            Selection is transient and not serialized."
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
