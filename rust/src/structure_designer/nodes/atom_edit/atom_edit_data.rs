use super::diff_recorder::{AtomDelta, AtomState, BondDelta, DiffRecorder};
use super::text_format::{parse_diff_text, serialize_diff};
use super::types::*;
use crate::api::common_api_types::SelectModifier;
use crate::api::structure_designer::structure_designer_api_types::APIAtomEditTool;
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::crystolecule::atomic_structure::{AtomicStructure, BondReference};
use crate::crystolecule::atomic_structure_diff::{
    AtomSource, DiffProvenance, apply_diff, enrich_diff_with_base_bonds,
};
use crate::crystolecule::motif::{Motif, MotifBond, Site, SiteSpecifier};
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::{EvalOutput, NodeData};
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::node_type::{NodeType, OutputPinDefinition, Parameter};
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use crate::util::transform::Transform;
use glam::IVec3;
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
    /// When true, run steepest descent minimization each frame during atom dragging
    pub continuous_minimization: bool,
    // Transient (NOT serialized)
    /// Current selection state
    pub selection: AtomEditSelection,
    /// Current editing tool
    pub active_tool: AtomEditTool,
    /// Shared element selection across all tools (Default, AddAtom).
    /// Survives tool switches — set once, used everywhere.
    pub selected_atomic_number: i16,
    /// Last known diff stats (updated during eval, used by get_subtitle)
    last_stats: Option<crate::crystolecule::atomic_structure_diff::DiffStats>,
    /// Cached input molecule for interactive editing performance.
    /// When present, reused instead of re-evaluating upstream.
    /// Cleared by `clear_input_cache()` when upstream may have changed.
    cached_input: Mutex<Option<AtomicStructure>>,
    /// Result-space atom ID to highlight while the modify measurement dialog is open.
    /// Set by `atom_edit_set_measurement_mark`, cleared by `atom_edit_clear_measurement_mark`.
    pub measurement_marked_atom_id: Option<u32>,
    /// Active recorder. When Some, mutations are recorded for undo/redo.
    pub(super) recorder: Option<DiffRecorder>,

    // --- Motif mode fields (dual-registration pattern) ---
    /// True for motif_edit nodes, false for atom_edit nodes.
    /// Controls eval output type and display override behavior.
    pub is_motif_mode: bool,
    /// Parameter element definitions: (name, default_atomic_number).
    /// e.g., [("PRIMARY", 6), ("SECONDARY", 14)]
    /// Only meaningful when is_motif_mode = true.
    pub parameter_elements: Vec<(String, i16)>,
    /// How far into neighboring cells to show ghost atoms (0.0–1.0).
    /// 0.0 = no ghosts, 1.0 = full neighboring cells.
    /// Default: 0.3 (covers diamond-family cross-cell bonding).
    /// Only used when is_motif_mode = true.
    pub neighbor_depth: f64,
    /// Cross-cell bond metadata: maps a bond (in the diff AtomicStructure)
    /// to the relative_cell offset. The stored IVec3 is the offset of
    /// max(atom_id1, atom_id2) relative to min(atom_id1, atom_id2).
    /// Bonds not in this map are same-cell (relative_cell = IVec3::ZERO).
    /// Only used when is_motif_mode = true.
    pub cross_cell_bonds: HashMap<BondReference, IVec3>,
    /// Cached unit cell for interactive editing (avoids re-evaluating upstream).
    /// Populated during eval(), read by tools during interaction.
    /// Transient — not serialized.
    pub cached_unit_cell: Mutex<Option<UnitCellStruct>>,
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
            show_anchor_arrows: true,
            include_base_bonds_in_diff: true,
            tolerance: DEFAULT_TOLERANCE,
            error_on_stale_entries: false,
            continuous_minimization: false,
            selection: AtomEditSelection::new(),
            active_tool: AtomEditTool::Default(DefaultToolState {
                interaction_state: DefaultToolInteractionState::default(),
                show_gadget: false,
            }),
            selected_atomic_number: 6, // Default to carbon
            last_stats: None,
            cached_input: Mutex::new(None),
            measurement_marked_atom_id: None,
            recorder: None,
            is_motif_mode: false,
            parameter_elements: Vec::new(),
            neighbor_depth: 0.3,
            cross_cell_bonds: HashMap::new(),
            cached_unit_cell: Mutex::new(None),
        }
    }

    /// Creates a new AtomEditData configured for motif_edit mode.
    pub fn new_motif_mode() -> Self {
        let mut data = Self::new();
        data.is_motif_mode = true;
        data
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
        continuous_minimization: bool,
        is_motif_mode: bool,
        parameter_elements: Vec<(String, i16)>,
        neighbor_depth: f64,
        cross_cell_bonds: HashMap<BondReference, IVec3>,
    ) -> Self {
        Self {
            diff,
            output_diff,
            show_anchor_arrows,
            include_base_bonds_in_diff,
            tolerance,
            error_on_stale_entries,
            continuous_minimization,
            measurement_marked_atom_id: None,
            selection: AtomEditSelection::new(),
            active_tool: AtomEditTool::Default(DefaultToolState {
                interaction_state: DefaultToolInteractionState::default(),
                show_gadget: false,
            }),
            selected_atomic_number: 6,
            last_stats: None,
            cached_input: Mutex::new(None),
            recorder: None,
            is_motif_mode,
            parameter_elements,
            neighbor_depth,
            cross_cell_bonds,
            cached_unit_cell: Mutex::new(None),
        }
    }

    /// Returns the pin index for the tolerance input.
    /// atom_edit: pin 1; motif_edit: pin 2 (unit_cell is pin 1).
    fn tolerance_pin_index(&self) -> usize {
        if self.is_motif_mode { 2 } else { 1 }
    }

    /// Motif-mode evaluation path. Produces a Motif on pin 0 (wire) with an
    /// Atomic display override for the viewport, and Atomic diff on pin 1.
    fn eval_motif_mode<'a>(
        &self,
        network_evaluator: &NetworkEvaluator,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &crate::structure_designer::node_type_registry::NodeTypeRegistry,
        decorate: bool,
        context: &mut crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext,
        input_structure: AtomicStructure,
        tolerance: f64,
    ) -> EvalOutput {
        // 1. Get unit cell from pin 1
        let unit_cell_val =
            network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 1);
        let unit_cell = match unit_cell_val {
            NetworkResult::UnitCell(uc) => uc,
            NetworkResult::None => {
                return EvalOutput::single(NetworkResult::Error(
                    "unit_cell input required".to_string(),
                ));
            }
            NetworkResult::Error(e) => return EvalOutput::single(NetworkResult::Error(e)),
            _ => {
                return EvalOutput::single(NetworkResult::Error(
                    "unit_cell: wrong type".to_string(),
                ));
            }
        };

        // Cache unit cell for interactive tools
        if let Ok(mut guard) = self.cached_unit_cell.lock() {
            *guard = Some(unit_cell.clone());
        }

        // 2. Apply diff (identical to atom_edit)
        let diff_result = apply_diff(&input_structure, &self.diff, tolerance);

        // Error on stale entries check (same as atom_edit)
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
                let error_msg = format!("Stale entries: {}", parts.join(", "));
                if network_stack.len() == 1 {
                    let eval_cache = AtomEditEvalCache {
                        provenance: diff_result.provenance,
                        stats: diff_result.stats,
                    };
                    context.selected_node_eval_cache = Some(Box::new(eval_cache));
                }
                return EvalOutput::single(NetworkResult::Error(error_msg));
            }
        }

        let mut result = diff_result.result;

        // 3. Convert AtomicStructure → Motif
        let motif = atomic_structure_to_motif(
            &result,
            &unit_cell,
            &self.parameter_elements,
            &self.cross_cell_bonds,
        );

        // 3b. Populate element name overrides on the display result so hover
        //     tooltips show user-defined parameter names (e.g., "PRIMARY")
        //     instead of "Unknown".
        for (i, (name, _)) in self.parameter_elements.iter().enumerate() {
            let reserved_z = super::types::param_index_to_atomic_number(i);
            result
                .decorator_mut()
                .element_name_overrides
                .insert(reserved_z, name.clone());
        }

        // 4. Build diff output (pin 1) — same as atom_edit
        let mut diff_clone = self.diff.clone();
        if self.include_base_bonds_in_diff {
            enrich_diff_with_base_bonds(&mut diff_clone, &input_structure, tolerance);
        }
        diff_clone.decorator_mut().show_anchor_arrows = self.show_anchor_arrows;
        if decorate {
            diff_clone.decorator_mut().from_selected_node = true;
            for &diff_id in &self.selection.selected_diff_atoms {
                diff_clone.set_atom_selected(diff_id, true);
            }
            for bond_ref in &self.selection.selected_bonds {
                diff_clone.decorator_mut().select_bond(bond_ref);
            }
            if let Some(ref transform) = self.selection.selection_transform {
                diff_clone.decorator_mut().selection_transform = Some(transform.clone());
            }
            if let Some(mark_id) = self.measurement_marked_atom_id {
                diff_clone.decorator_mut().set_atom_display_state(
                    mark_id,
                    crate::crystolecule::atomic_structure::AtomDisplayState::Marked,
                );
            }
            self.apply_guided_placement_decoration(&mut diff_clone, None);
        }

        // 5. Build display visualization (pin 0 display override)
        if decorate {
            result.decorator_mut().from_selected_node = true;
            for &base_id in &self.selection.selected_base_atoms {
                if let Some(&result_id) = diff_result.provenance.base_to_result.get(&base_id) {
                    result.set_atom_selected(result_id, true);
                }
            }
            for &diff_id in &self.selection.selected_diff_atoms {
                if let Some(&result_id) = diff_result.provenance.diff_to_result.get(&diff_id) {
                    result.set_atom_selected(result_id, true);
                }
            }
            for bond_ref in &self.selection.selected_bonds {
                result.decorator_mut().select_bond(bond_ref);
            }
            if let Some(ref transform) = self.selection.selection_transform {
                result.decorator_mut().selection_transform = Some(transform.clone());
            }
            if let AtomEditTool::AddBond(state) = &self.active_tool {
                let mark_diff_id = match &state.interaction_state {
                    AddBondInteractionState::Pending { hit_atom_id, .. } => Some(*hit_atom_id),
                    AddBondInteractionState::Dragging { source_atom_id, .. } => {
                        Some(*source_atom_id)
                    }
                    AddBondInteractionState::Idle => state.last_atom_id,
                };
                if let Some(diff_id) = mark_diff_id {
                    if let Some(&result_id) = diff_result.provenance.diff_to_result.get(&diff_id) {
                        result.decorator_mut().set_atom_display_state(
                            result_id,
                            crate::crystolecule::atomic_structure::AtomDisplayState::Marked,
                        );
                    }
                }
            }
            if let Some(mark_id) = self.measurement_marked_atom_id {
                result.decorator_mut().set_atom_display_state(
                    mark_id,
                    crate::crystolecule::atomic_structure::AtomDisplayState::Marked,
                );
            }
            self.apply_guided_placement_decoration(&mut result, Some(&diff_result.provenance));
        }

        // 5b. Generate ghost atoms for neighboring cells
        if self.neighbor_depth > 0.0 {
            generate_ghost_atoms(
                &mut result,
                &unit_cell,
                self.neighbor_depth,
                &self.cross_cell_bonds,
            );
        }

        // 6. Store eval cache
        if network_stack.len() == 1 {
            let eval_cache = AtomEditEvalCache {
                provenance: diff_result.provenance,
                stats: diff_result.stats,
            };
            context.selected_node_eval_cache = Some(Box::new(eval_cache));
        }

        // 7. Build EvalOutput with display override
        let mut output = EvalOutput::multi(vec![
            NetworkResult::Motif(motif),       // pin 0 wire value
            NetworkResult::Atomic(diff_clone), // pin 1
        ]);
        output.set_display_override(0, NetworkResult::Atomic(result)); // pin 0 display
        output.unit_cell_override = Some(unit_cell);

        output
    }

    // --- Recording methods ---

    /// Start recording diff mutations for undo/redo.
    pub fn begin_recording(&mut self) {
        self.recorder = Some(DiffRecorder::default());
    }

    /// End recording and return the accumulated deltas.
    /// Coalesces redundant deltas before returning.
    pub fn end_recording(&mut self) -> Option<DiffRecorder> {
        let mut recorder = self.recorder.take();
        if let Some(ref mut rec) = recorder {
            rec.coalesce();
        }
        recorder
    }

    // --- Recorded wrapper methods for direct AtomicStructure mutations ---
    // Used by code paths that call self.diff.* directly (apply_replace, apply_transform,
    // operations.rs, minimization.rs, hydrogen_passivation.rs, modify_measurement.rs).

    /// Set an atom's atomic_number with recording.
    pub fn set_atomic_number_recorded(&mut self, atom_id: u32, atomic_number: i16) {
        if let Some(ref mut rec) = self.recorder {
            if let Some(atom) = self.diff.get_atom(atom_id) {
                let old_an = atom.atomic_number;
                let pos = atom.position;
                let flags = atom.flags;
                let anchor = self.diff.anchor_position(atom_id).copied();
                if old_an != atomic_number {
                    rec.atom_deltas.push(AtomDelta {
                        atom_id,
                        before: Some(AtomState {
                            atomic_number: old_an,
                            position: pos,
                            anchor,
                            flags,
                        }),
                        after: Some(AtomState {
                            atomic_number,
                            position: pos,
                            anchor,
                            flags,
                        }),
                    });
                }
            }
        }
        self.diff.set_atomic_number(atom_id, atomic_number);
    }

    /// Set an atom's anchor position with recording.
    pub fn set_anchor_recorded(&mut self, atom_id: u32, anchor: DVec3) {
        if let Some(ref mut rec) = self.recorder {
            if let Some(atom) = self.diff.get_atom(atom_id) {
                let old_anchor = self.diff.anchor_position(atom_id).copied();
                if old_anchor != Some(anchor) {
                    rec.atom_deltas.push(AtomDelta {
                        atom_id,
                        before: Some(AtomState {
                            atomic_number: atom.atomic_number,
                            position: atom.position,
                            anchor: old_anchor,
                            flags: atom.flags,
                        }),
                        after: Some(AtomState {
                            atomic_number: atom.atomic_number,
                            position: atom.position,
                            anchor: Some(anchor),
                            flags: atom.flags,
                        }),
                    });
                }
            }
        }
        self.diff.set_anchor_position(atom_id, anchor);
    }

    /// Add atom directly to diff with recording. For use by code that
    /// bypasses add_atom_to_diff (minimization, hydrogen passivation, etc.)
    pub fn add_atom_recorded(&mut self, atomic_number: i16, position: DVec3) -> u32 {
        let id = self.diff.add_atom(atomic_number, position);
        if let Some(ref mut rec) = self.recorder {
            rec.atom_deltas.push(AtomDelta {
                atom_id: id,
                before: None,
                after: Some(AtomState {
                    atomic_number,
                    position,
                    anchor: None,
                    flags: 0,
                }),
            });
        }
        id
    }

    /// Add bond with recording. For use by code that calls diff.add_bond_checked directly.
    pub fn add_bond_recorded(&mut self, atom_id1: u32, atom_id2: u32, order: u8) {
        let old_order = self.diff.get_atom(atom_id1).and_then(|a| {
            a.bonds
                .iter()
                .find(|b| b.other_atom_id() == atom_id2)
                .map(|b| b.bond_order())
        });
        self.diff.add_bond_checked(atom_id1, atom_id2, order);
        if let Some(ref mut rec) = self.recorder {
            let (a, b) = if atom_id1 < atom_id2 {
                (atom_id1, atom_id2)
            } else {
                (atom_id2, atom_id1)
            };
            rec.bond_deltas.push(BondDelta {
                atom_id1: a,
                atom_id2: b,
                old_order,
                new_order: Some(order),
            });
        }
    }

    /// Delete a bond with recording. For use by apply_delete_diff_view.
    pub fn delete_bond_recorded(&mut self, bond_ref: &BondReference) {
        let old_order = self.diff.get_atom(bond_ref.atom_id1).and_then(|a| {
            a.bonds
                .iter()
                .find(|b| b.other_atom_id() == bond_ref.atom_id2)
                .map(|b| b.bond_order())
        });
        self.diff.delete_bond(bond_ref);
        if let Some(ref mut rec) = self.recorder {
            if let Some(order) = old_order {
                let (a, b) = if bond_ref.atom_id1 < bond_ref.atom_id2 {
                    (bond_ref.atom_id1, bond_ref.atom_id2)
                } else {
                    (bond_ref.atom_id2, bond_ref.atom_id1)
                };
                rec.bond_deltas.push(BondDelta {
                    atom_id1: a,
                    atom_id2: b,
                    old_order: Some(order),
                    new_order: None,
                });
            }
        }
    }

    /// Set atom position with recording. For use by minimization etc.
    pub fn set_position_recorded(&mut self, atom_id: u32, new_position: DVec3) {
        if let Some(ref mut rec) = self.recorder {
            if let Some(atom) = self.diff.get_atom(atom_id) {
                let anchor = self.diff.anchor_position(atom_id).copied();
                rec.atom_deltas.push(AtomDelta {
                    atom_id,
                    before: Some(AtomState {
                        atomic_number: atom.atomic_number,
                        position: atom.position,
                        anchor,
                        flags: atom.flags,
                    }),
                    after: Some(AtomState {
                        atomic_number: atom.atomic_number,
                        position: new_position,
                        anchor,
                        flags: atom.flags,
                    }),
                });
            }
        }
        self.diff.set_atom_position(atom_id, new_position);
    }

    /// Set all non-selection flags on a diff atom with recording.
    /// Used by `promote_base_atom_metadata` (future Phase 2) to copy base atom flags
    /// to the new diff atom within a recording session.
    pub fn set_flags_recorded(&mut self, atom_id: u32, flags: u16) {
        if let Some(ref mut rec) = self.recorder {
            if let Some(atom) = self.diff.get_atom(atom_id) {
                let old_flags = atom.flags;
                // Compare non-selection bits only
                if (old_flags & !0x1) != (flags & !0x1) {
                    let anchor = self.diff.anchor_position(atom_id).copied();
                    rec.atom_deltas.push(AtomDelta {
                        atom_id,
                        before: Some(AtomState {
                            atomic_number: atom.atomic_number,
                            position: atom.position,
                            anchor,
                            flags: old_flags,
                        }),
                        after: Some(AtomState {
                            atomic_number: atom.atomic_number,
                            position: atom.position,
                            anchor,
                            flags: (old_flags & 0x1) | (flags & !0x1),
                        }),
                    });
                }
            }
        }
        // Apply: preserve selection bit, set everything else
        if let Some(atom) = self.diff.get_atom(atom_id) {
            let selected = atom.flags & 0x1;
            let new_flags = selected | (flags & !0x1);
            // Use per-flag setters since get_atom_mut is private
            self.diff
                .set_atom_frozen(atom_id, (new_flags & (1 << 2)) != 0);
            self.diff
                .set_atom_hydrogen_passivation(atom_id, (new_flags & (1 << 1)) != 0);
            self.diff
                .set_atom_hybridization_override(atom_id, ((new_flags >> 3) & 0b11) as u8);
        }
    }

    /// Set the frozen flag on a diff atom with recording.
    pub fn set_frozen_recorded(&mut self, atom_id: u32, frozen: bool) {
        if let Some(ref mut rec) = self.recorder {
            if let Some(atom) = self.diff.get_atom(atom_id) {
                if atom.is_frozen() != frozen {
                    let anchor = self.diff.anchor_position(atom_id).copied();
                    let mut new_flags = atom.flags;
                    if frozen {
                        new_flags |= 1 << 2;
                    } else {
                        new_flags &= !(1 << 2);
                    }
                    rec.atom_deltas.push(AtomDelta {
                        atom_id,
                        before: Some(AtomState {
                            atomic_number: atom.atomic_number,
                            position: atom.position,
                            anchor,
                            flags: atom.flags,
                        }),
                        after: Some(AtomState {
                            atomic_number: atom.atomic_number,
                            position: atom.position,
                            anchor,
                            flags: new_flags,
                        }),
                    });
                }
            }
        }
        self.diff.set_atom_frozen(atom_id, frozen);
    }

    /// Set the hybridization override on a diff atom with recording.
    pub fn set_hybridization_override_recorded(&mut self, atom_id: u32, hybridization: u8) {
        if let Some(ref mut rec) = self.recorder {
            if let Some(atom) = self.diff.get_atom(atom_id) {
                if atom.hybridization_override() != hybridization {
                    let anchor = self.diff.anchor_position(atom_id).copied();
                    let mut new_flags = atom.flags;
                    // Clear hybridization bits and set new value
                    new_flags = (new_flags & !(0b11 << 3)) | (((hybridization as u16) & 0b11) << 3);
                    rec.atom_deltas.push(AtomDelta {
                        atom_id,
                        before: Some(AtomState {
                            atomic_number: atom.atomic_number,
                            position: atom.position,
                            anchor,
                            flags: atom.flags,
                        }),
                        after: Some(AtomState {
                            atomic_number: atom.atomic_number,
                            position: atom.position,
                            anchor,
                            flags: new_flags,
                        }),
                    });
                }
            }
        }
        self.diff
            .set_atom_hybridization_override(atom_id, hybridization);
    }

    // --- Cross-cell bond methods ---

    /// Record a cross-cell bond entry being set. The offset follows the
    /// normalization convention: IVec3 is the cell offset of max(id1,id2)
    /// relative to min(id1,id2).
    pub fn set_cross_cell_bond_recorded(&mut self, bond_ref: BondReference, offset: IVec3) {
        use super::diff_recorder::CrossCellBondDelta;
        let old_offset = self.cross_cell_bonds.get(&bond_ref).copied();
        self.cross_cell_bonds.insert(bond_ref.clone(), offset);
        if let Some(ref mut rec) = self.recorder {
            rec.cross_cell_bond_deltas.push(CrossCellBondDelta {
                bond_ref,
                old_offset,
                new_offset: Some(offset),
            });
        }
    }

    /// Remove a cross-cell bond entry with recording.
    pub fn remove_cross_cell_bond_recorded(&mut self, bond_ref: &BondReference) {
        use super::diff_recorder::CrossCellBondDelta;
        let old_offset = self.cross_cell_bonds.remove(bond_ref);
        if let Some(ref mut rec) = self.recorder {
            if let Some(offset) = old_offset {
                rec.cross_cell_bond_deltas.push(CrossCellBondDelta {
                    bond_ref: bond_ref.clone(),
                    old_offset: Some(offset),
                    new_offset: None,
                });
            }
        }
    }

    // --- Bulk merge ---

    /// Merge all atoms and bonds from an external AtomicStructure into the diff
    /// as pure additions (no anchors). Returns the list of new diff atom IDs.
    ///
    /// This is used by "Import XYZ" in direct editing mode to bake imported atoms
    /// directly into the edit layer rather than wiring an import_xyz node.
    pub fn merge_atomic_structure(&mut self, structure: &AtomicStructure) -> Vec<u32> {
        use rustc_hash::FxHashMap;

        let mut id_map: FxHashMap<u32, u32> = FxHashMap::default();
        let mut added_ids = Vec::new();

        // Phase 1: Add all atoms
        for (&ext_id, atom) in structure.iter_atoms() {
            let new_id = self.add_atom_recorded(atom.atomic_number, atom.position);
            id_map.insert(ext_id, new_id);
            added_ids.push(new_id);
        }

        // Phase 2: Add all bonds (deduplicated — only when atom_id < other_id)
        for (&ext_id, atom) in structure.iter_atoms() {
            for bond in &atom.bonds {
                let other_ext_id = bond.other_atom_id();
                if ext_id < other_ext_id {
                    if let (Some(&new_id1), Some(&new_id2)) =
                        (id_map.get(&ext_id), id_map.get(&other_ext_id))
                    {
                        self.add_bond_recorded(new_id1, new_id2, bond.bond_order());
                    }
                }
            }
        }

        added_ids
    }

    // --- Promotion helpers ---

    /// Copy per-atom metadata (flags) from a base atom to its promoted diff atom.
    /// Must be called at every promotion site alongside selection migration.
    /// Uses set_flags_recorded so the flag copy is captured in the undo delta.
    pub fn promote_base_atom_metadata(&mut self, base_atom_flags: u16, diff_id: u32) {
        // Copy all flags except selection (bit 0)
        let flags = base_atom_flags & !0x1;
        if flags != 0 {
            self.set_flags_recorded(diff_id, flags);
        }
    }

    // --- Direct diff mutation methods ---

    /// Add an atom to the diff at the given position.
    /// Returns the new atom's ID in the diff.
    pub fn add_atom_to_diff(&mut self, atomic_number: i16, position: DVec3) -> u32 {
        self.selection.clear_bonds();
        let id = self.diff.add_atom(atomic_number, position);
        if let Some(ref mut rec) = self.recorder {
            rec.atom_deltas.push(AtomDelta {
                atom_id: id,
                before: None,
                after: Some(AtomState {
                    atomic_number,
                    position,
                    anchor: None,
                    flags: 0,
                }),
            });
        }
        id
    }

    /// Add a delete marker at the given position.
    /// Returns the delete marker's ID in the diff.
    pub fn mark_for_deletion(&mut self, match_position: DVec3) -> u32 {
        self.selection.clear_bonds();
        let atomic_number = crate::crystolecule::atomic_structure::DELETED_SITE_ATOMIC_NUMBER;
        let id = self.diff.add_atom(atomic_number, match_position);
        if let Some(ref mut rec) = self.recorder {
            rec.atom_deltas.push(AtomDelta {
                atom_id: id,
                before: None,
                after: Some(AtomState {
                    atomic_number,
                    position: match_position,
                    anchor: None,
                    flags: 0,
                }),
            });
        }
        id
    }

    /// Add/update an atom in the diff with a new atomic number at the given position.
    /// Sets an anchor at match_position so that apply_diff (and compose_two_diffs)
    /// correctly identifies this as a base-atom modification rather than a pure addition.
    /// Returns the atom's ID in the diff.
    pub fn replace_in_diff(&mut self, match_position: DVec3, new_atomic_number: i16) -> u32 {
        self.selection.clear_bonds();
        let id = self.diff.add_atom(new_atomic_number, match_position);
        self.diff.set_anchor_position(id, match_position);
        if let Some(ref mut rec) = self.recorder {
            rec.atom_deltas.push(AtomDelta {
                atom_id: id,
                before: None,
                after: Some(AtomState {
                    atomic_number: new_atomic_number,
                    position: match_position,
                    anchor: Some(match_position),
                    flags: 0,
                }),
            });
        }
        id
    }

    /// Move an atom that is already in the diff to a new position.
    ///
    /// IMPORTANT: This method MUST NOT set anchor positions. Anchors are only
    /// set at promotion time — when a base atom is first added to the diff via
    /// `add_atom` + `set_anchor_position`. Pure addition atoms (atoms created
    /// by AddAtom that have no base counterpart) must never receive an anchor,
    /// because `apply_diff` treats anchored-but-unmatched atoms as "orphaned
    /// tracked atoms" and drops them from the result.
    pub fn move_in_diff(&mut self, atom_id: u32, new_position: DVec3) {
        // Capture old state for recording
        let old_state = if self.recorder.is_some() {
            self.diff.get_atom(atom_id).map(|a| AtomState {
                atomic_number: a.atomic_number,
                position: a.position,
                anchor: self.diff.anchor_position(atom_id).copied(),
                flags: a.flags,
            })
        } else {
            None
        };

        self.selection.clear_bonds();
        self.diff.set_atom_position(atom_id, new_position);

        if let Some(ref mut rec) = self.recorder {
            if let Some(old) = old_state {
                rec.atom_deltas.push(AtomDelta {
                    atom_id,
                    before: Some(old.clone()),
                    after: Some(AtomState {
                        position: new_position,
                        ..old
                    }),
                });
            }
        }
    }

    /// Add a bond between two atoms in the diff.
    pub fn add_bond_in_diff(&mut self, atom_id1: u32, atom_id2: u32, order: u8) {
        // Capture old bond state for recording
        let old_order = if self.recorder.is_some() {
            self.diff.get_atom(atom_id1).and_then(|a| {
                a.bonds
                    .iter()
                    .find(|b| b.other_atom_id() == atom_id2)
                    .map(|b| b.bond_order())
            })
        } else {
            None
        };

        self.selection.clear_bonds();
        self.diff.add_bond_checked(atom_id1, atom_id2, order);

        if let Some(ref mut rec) = self.recorder {
            let (a, b) = if atom_id1 < atom_id2 {
                (atom_id1, atom_id2)
            } else {
                (atom_id2, atom_id1)
            };
            rec.bond_deltas.push(BondDelta {
                atom_id1: a,
                atom_id2: b,
                old_order,
                new_order: Some(order),
            });
        }
    }

    /// Add a bond delete marker between two atoms in the diff.
    /// Ensures both atoms are present in the diff (adds identity entries if needed).
    pub fn delete_bond_in_diff(&mut self, atom_id1: u32, atom_id2: u32) {
        // Capture old bond state for recording
        let old_order = if self.recorder.is_some() {
            self.diff.get_atom(atom_id1).and_then(|a| {
                a.bonds
                    .iter()
                    .find(|b| b.other_atom_id() == atom_id2)
                    .map(|b| b.bond_order())
            })
        } else {
            None
        };

        self.selection.clear_bonds();
        let order = crate::crystolecule::atomic_structure::BOND_DELETED;
        self.diff.add_bond(atom_id1, atom_id2, order);

        if let Some(ref mut rec) = self.recorder {
            let (a, b) = if atom_id1 < atom_id2 {
                (atom_id1, atom_id2)
            } else {
                (atom_id2, atom_id1)
            };
            rec.bond_deltas.push(BondDelta {
                atom_id1: a,
                atom_id2: b,
                old_order,
                new_order: Some(order),
            });
        }

        // Also remove any cross-cell bond metadata for this bond
        let bond_ref = BondReference { atom_id1, atom_id2 };
        self.remove_cross_cell_bond_recorded(&bond_ref);
    }

    /// Get a clone of the cached input structure (if available).
    /// Used by bond order change operations to resolve result-space IDs.
    pub fn get_cached_input(&self) -> Option<AtomicStructure> {
        self.cached_input.lock().ok().and_then(|g| g.clone())
    }

    /// Remove an atom from the diff entirely (and its anchor if any).
    pub fn remove_from_diff(&mut self, diff_atom_id: u32) {
        // Capture before-state for recording
        let before_state = if self.recorder.is_some() {
            self.diff.get_atom(diff_atom_id).map(|a| {
                let anchor = self.diff.anchor_position(diff_atom_id).copied();
                let bonds: Vec<(u32, u8)> = a
                    .bonds
                    .iter()
                    .map(|b| (b.other_atom_id(), b.bond_order()))
                    .collect();
                (
                    AtomState {
                        atomic_number: a.atomic_number,
                        position: a.position,
                        anchor,
                        flags: a.flags,
                    },
                    bonds,
                )
            })
        } else {
            None
        };

        self.selection.clear_bonds();
        self.diff.delete_atom(diff_atom_id);
        self.diff.remove_anchor_position(diff_atom_id);

        // Record atom removal + bond removals
        if let Some(ref mut rec) = self.recorder {
            if let Some((atom_state, bonds)) = before_state {
                rec.atom_deltas.push(AtomDelta {
                    atom_id: diff_atom_id,
                    before: Some(atom_state),
                    after: None,
                });
                for (other_id, order) in bonds {
                    // Record bond removal with canonical ordering.
                    // No risk of double-recording: delete_atom removes bonds
                    // from both endpoints, so subsequent remove_from_diff calls
                    // on the other atom won't see this bond anymore.
                    let (a, b) = if diff_atom_id < other_id {
                        (diff_atom_id, other_id)
                    } else {
                        (other_id, diff_atom_id)
                    };
                    rec.bond_deltas.push(BondDelta {
                        atom_id1: a,
                        atom_id2: b,
                        old_order: Some(order),
                        new_order: None,
                    });
                }
            }
        }
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
                interaction_state: DefaultToolInteractionState::default(),
                show_gadget: false,
            }),
            APIAtomEditTool::AddAtom => AtomEditTool::AddAtom(AddAtomToolState::Idle),
            APIAtomEditTool::AddBond => AtomEditTool::AddBond(AddBondToolState {
                bond_order: crate::crystolecule::atomic_structure::BOND_SINGLE,
                interaction_state: AddBondInteractionState::default(),
                last_atom_id: None,
            }),
        }
    }

    /// Set the shared element selection. Updates the active tool's state as well:
    /// - Default tool: no additional action needed (reads from selected_atomic_number)
    /// - AddAtom tool: cancels guided placement (resets to Idle)
    pub fn set_selected_element(&mut self, atomic_number: i16) {
        self.selected_atomic_number = atomic_number;
        // Cancel guided placement when element changes
        if let AtomEditTool::AddAtom(_) = &self.active_tool {
            self.active_tool = AtomEditTool::AddAtom(AddAtomToolState::Idle);
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
                merge_targets,
                ..
            }) => {
                if let Some(output_id) = resolve_anchor(*anchor_atom_id) {
                    output
                        .decorator_mut()
                        .set_atom_display_state(output_id, AtomDisplayState::Marked);
                    let anchor_pos = output.get_atom(output_id).map(|a| a.position);
                    if let Some(anchor_pos) = anchor_pos {
                        let merge_flags: Vec<bool> =
                            merge_targets.iter().map(|mt| mt.is_some()).collect();
                        let merge_atom_ids: Vec<Option<u32>> = merge_targets
                            .iter()
                            .map(|mt| mt.as_ref().map(|t| t.result_atom_id))
                            .collect();
                        // Mark merge target atoms with magenta rim highlight
                        for &atom_id in merge_atom_ids.iter().flatten() {
                            output
                                .decorator_mut()
                                .set_atom_display_state(atom_id, AtomDisplayState::Marked);
                        }
                        output.decorator_mut().guide_placement_visuals =
                            Some(GuidePlacementVisuals {
                                anchor_pos,
                                guide_dots: guide_dots.clone(),
                                wireframe_sphere: None,
                                wireframe_ring: None,
                                merge_dot_flags: merge_flags,
                                merge_target_atom_ids: merge_atom_ids,
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
                                merge_dot_flags: Vec::new(),
                                merge_target_atom_ids: Vec::new(),
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
                                merge_dot_flags: Vec::new(),
                                merge_target_atom_ids: Vec::new(),
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
            self.selection
                .untrack_selected(SelectionProvenance::Base, *base_id);
        }

        // Delete diff atoms
        for (diff_id, is_pure_addition) in diff_atoms {
            if *is_pure_addition {
                self.remove_from_diff(*diff_id);
            } else {
                self.convert_to_delete_marker(*diff_id);
            }
            self.selection.selected_diff_atoms.remove(diff_id);
            self.selection
                .untrack_selected(SelectionProvenance::Diff, *diff_id);
        }

        // Delete bonds (add bond delete markers)
        for info in bonds {
            let actual_a = match info.diff_id_a {
                Some(id) => id,
                None => match info.identity_a {
                    Some((_an, pos)) => self.add_atom_recorded(
                        crate::crystolecule::atomic_structure::UNCHANGED_ATOMIC_NUMBER,
                        pos,
                    ),
                    None => continue,
                },
            };
            let actual_b = match info.diff_id_b {
                Some(id) => id,
                None => match info.identity_b {
                    Some((_an, pos)) => self.add_atom_recorded(
                        crate::crystolecule::atomic_structure::UNCHANGED_ATOMIC_NUMBER,
                        pos,
                    ),
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
                // Pure addition or unchanged marker → remove entirely
                DiffAtomKind::PureAddition | DiffAtomKind::Unchanged => {
                    self.remove_from_diff(*diff_id);
                }
            }
            self.selection.selected_diff_atoms.remove(diff_id);
            self.selection
                .untrack_selected(SelectionProvenance::Diff, *diff_id);
        }

        // Bonds in diff view: remove the bond from the diff entirely
        for bond_ref in bonds {
            self.delete_bond_recorded(bond_ref);
        }

        self.selection.selected_bonds.clear();
        self.selection.selection_transform = None;
    }

    /// Apply element replacement to selected atoms.
    pub fn apply_replace(
        &mut self,
        atomic_number: i16,
        base_atoms: &[super::operations::BaseAtomPromotionInfo],
    ) {
        // Replace diff atoms (update atomic_number in place)
        let diff_ids: Vec<u32> = self.selection.selected_diff_atoms.iter().copied().collect();
        for diff_id in &diff_ids {
            self.set_atomic_number_recorded(*diff_id, atomic_number);
        }

        // Replace base atoms (add to diff with new element, or promote existing entry)
        for info in base_atoms {
            let diff_id = if let Some(existing_id) = info.existing_diff_id {
                // Reuse existing diff entry (e.g., UNCHANGED marker).
                // Set real atomic_number; anchor = position for replacement.
                self.set_atomic_number_recorded(existing_id, atomic_number);
                self.set_anchor_recorded(existing_id, info.position);
                existing_id
            } else {
                self.replace_in_diff(info.position, atomic_number)
            };
            self.selection.selected_base_atoms.remove(&info.base_id);
            self.selection.selected_diff_atoms.insert(diff_id);
            self.selection.update_order_provenance(
                SelectionProvenance::Base,
                info.base_id,
                SelectionProvenance::Diff,
                diff_id,
            );
            self.promote_base_atom_metadata(info.flags, diff_id);
        }

        self.selection.clear_bonds();
    }

    /// Apply a relative transform to selected atoms.
    pub fn apply_transform(
        &mut self,
        relative: &Transform,
        base_atoms: &[super::operations::BaseAtomPromotionInfo],
    ) {
        // Transform existing diff atoms, skipping frozen ones.
        let diff_ids: Vec<u32> = self
            .selection
            .selected_diff_atoms
            .iter()
            .filter(|&&id| !self.diff.get_atom(id).map_or(false, |a| a.is_frozen()))
            .copied()
            .collect();
        for diff_id in diff_ids {
            let new_position = if let Some(atom) = self.diff.get_atom(diff_id) {
                relative.apply_to_position(&atom.position)
            } else {
                continue;
            };
            self.move_in_diff(diff_id, new_position);
        }

        // Promote base atoms to diff with anchors at old positions.
        // Anchor is set here at promotion time so apply_diff can match them
        // back to the base atom. See "Anchor Invariant" in AGENTS.md.
        for info in base_atoms {
            let new_position = relative.apply_to_position(&info.position);
            let diff_id = if let Some(existing_id) = info.existing_diff_id {
                // Reuse existing diff entry (e.g., UNCHANGED marker).
                // Promote: set real atomic_number and anchor, then move.
                self.set_atomic_number_recorded(existing_id, info.atomic_number);
                self.set_anchor_recorded(existing_id, info.position);
                self.move_in_diff(existing_id, new_position);
                existing_id
            } else {
                let new_diff_id = self.add_atom_recorded(info.atomic_number, new_position);
                self.set_anchor_recorded(new_diff_id, info.position);
                new_diff_id
            };
            self.selection.selected_base_atoms.remove(&info.base_id);
            self.selection.selected_diff_atoms.insert(diff_id);
            self.selection.update_order_provenance(
                SelectionProvenance::Base,
                info.base_id,
                SelectionProvenance::Diff,
                diff_id,
            );
            self.promote_base_atom_metadata(info.flags, diff_id);
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

        // Gather diff atom positions directly from the diff, skipping frozen atoms.
        let mut diff_atom_positions: Vec<(u32, DVec3)> = Vec::new();
        for &diff_id in &self.selection.selected_diff_atoms {
            if self.diff.get_atom(diff_id).map_or(false, |a| a.is_frozen()) {
                continue;
            }
            if let Some(atom) = self.diff.get_atom(diff_id) {
                diff_atom_positions.push((diff_id, atom.position));
            }
        }

        // Gather base atom info (needs eval cache for provenance → result positions).
        let mut base_atoms_info: Vec<(u32, i16, DVec3, Option<u32>, u16)> = Vec::new();
        if !self.selection.selected_base_atoms.is_empty()
            && !structure_designer.is_selected_node_in_diff_view()
        {
            if let Some(eval_cache) = structure_designer.get_selected_node_eval_cache() {
                if let Some(cache) = eval_cache.downcast_ref::<AtomEditEvalCache>() {
                    if let Some(result) =
                        structure_designer.get_atomic_structure_from_selected_node()
                    {
                        for &base_id in &self.selection.selected_base_atoms {
                            if let Some(&result_id) = cache.provenance.base_to_result.get(&base_id)
                            {
                                if let Some(atom) = result.get_atom(result_id) {
                                    // Skip frozen atoms (flag flows through apply_diff)
                                    if atom.is_frozen() {
                                        continue;
                                    }
                                    // Check if this base atom already has a diff entry
                                    let existing_diff_id = match cache
                                        .provenance
                                        .sources
                                        .get(&result_id)
                                    {
                                        Some(
                                            crate::crystolecule::atomic_structure_diff::AtomSource::DiffMatchedBase {
                                                diff_id,
                                                ..
                                            },
                                        ) => Some(*diff_id),
                                        _ => None,
                                    };
                                    base_atoms_info.push((
                                        base_id,
                                        atom.atomic_number,
                                        atom.position,
                                        existing_diff_id,
                                        atom.flags,
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
    ) -> EvalOutput {
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
                return EvalOutput::single(input_val);
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

        // Get tolerance from tolerance pin or property
        let tolerance = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            self.tolerance_pin_index(),
            self.tolerance,
            NetworkResult::extract_float,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        // Dispatch to motif mode if applicable
        if self.is_motif_mode {
            return self.eval_motif_mode(
                network_evaluator,
                network_stack,
                node_id,
                registry,
                decorate,
                context,
                input_structure,
                tolerance,
            );
        }

        // Apply the diff to the input
        let diff_result = apply_diff(&input_structure, &self.diff, tolerance);

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
                let error_msg = format!("Stale entries: {}", parts.join(", "));
                // Store eval cache before returning error so the panel can still show diagnostics
                if network_stack.len() == 1 {
                    let eval_cache = AtomEditEvalCache {
                        provenance: diff_result.provenance,
                        stats: diff_result.stats,
                    };
                    context.selected_node_eval_cache = Some(Box::new(eval_cache));
                }
                return EvalOutput::single(NetworkResult::Error(error_msg));
            }
        }

        let mut result = diff_result.result;
        // Frozen/hybridization flags now flow through apply_diff automatically via
        // copy_atom_metadata from diff atoms. No manual provenance-mapping loops needed.

        // --- Pin 1 (diff): build diff output with inherent decorations ---
        let mut diff_clone = self.diff.clone();
        if self.include_base_bonds_in_diff {
            enrich_diff_with_base_bonds(&mut diff_clone, &input_structure, tolerance);
        }
        // Flags are already on diff atoms — no manual application needed.
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

            // Mark measurement-dialog atom (in diff view, ID is used directly)
            if let Some(mark_id) = self.measurement_marked_atom_id {
                diff_clone.decorator_mut().set_atom_display_state(
                    mark_id,
                    crate::crystolecule::atomic_structure::AtomDisplayState::Marked,
                );
            }

            // Mark guided placement anchor and store guide visuals
            self.apply_guided_placement_decoration(&mut diff_clone, None);
        }

        // --- Pin 0 (result): apply selection/tool decorations ---
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
                    if let Some(&result_id) = diff_result.provenance.diff_to_result.get(&diff_id) {
                        result.decorator_mut().set_atom_display_state(
                            result_id,
                            crate::crystolecule::atomic_structure::AtomDisplayState::Marked,
                        );
                    }
                }
            }

            // Mark measurement-dialog atom (result-space ID, applied directly)
            if let Some(mark_id) = self.measurement_marked_atom_id {
                result.decorator_mut().set_atom_display_state(
                    mark_id,
                    crate::crystolecule::atomic_structure::AtomDisplayState::Marked,
                );
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

        EvalOutput::multi(vec![
            NetworkResult::Atomic(result),
            NetworkResult::Atomic(diff_clone),
        ])
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(AtomEditData {
            diff: self.diff.clone(),
            output_diff: self.output_diff,
            show_anchor_arrows: self.show_anchor_arrows,
            include_base_bonds_in_diff: self.include_base_bonds_in_diff,
            tolerance: self.tolerance,
            error_on_stale_entries: self.error_on_stale_entries,
            continuous_minimization: self.continuous_minimization,
            selection: self.selection.clone(),
            active_tool: match &self.active_tool {
                AtomEditTool::Default(state) => AtomEditTool::Default(DefaultToolState {
                    interaction_state: DefaultToolInteractionState::default(),
                    show_gadget: state.show_gadget,
                }),
                AtomEditTool::AddAtom(_) => AtomEditTool::AddAtom(AddAtomToolState::Idle),
                AtomEditTool::AddBond(state) => AtomEditTool::AddBond(AddBondToolState {
                    bond_order: state.bond_order,
                    interaction_state: AddBondInteractionState::default(),
                    last_atom_id: state.last_atom_id,
                }),
            },
            selected_atomic_number: self.selected_atomic_number,
            last_stats: self.last_stats.clone(),
            cached_input: Mutex::new(None),
            measurement_marked_atom_id: self.measurement_marked_atom_id,
            recorder: None, // Never clone an active recorder
            is_motif_mode: self.is_motif_mode,
            parameter_elements: self.parameter_elements.clone(),
            neighbor_depth: self.neighbor_depth,
            cross_cell_bonds: self.cross_cell_bonds.clone(),
            cached_unit_cell: Mutex::new(None),
        })
    }

    fn clear_input_cache(&self) {
        if let Ok(mut guard) = self.cached_input.lock() {
            *guard = None;
        }
    }

    fn get_subtitle(&self, connected_input_pins: &HashSet<String>) -> Option<String> {
        // Use last known stats if available (updated during eval)
        let stats_part = if let Some(stats) = &self.last_stats {
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
        };

        // Append tolerance when tolerance pin is not connected
        if !connected_input_pins.contains("tolerance") {
            let tol_part = format!("tol={:.3}", self.tolerance);
            match stats_part {
                Some(s) => Some(format!("{s}, {tol_part}")),
                None => Some(tol_part),
            }
        } else {
            stats_part
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
        if self.is_motif_mode {
            m.insert("unit_cell".to_string(), (false, None)); // optional but needed for motif output
        }
        m.insert("tolerance".to_string(), (false, None)); // optional: overrides property
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
        parameters: vec![
            Parameter {
                id: None,
                name: "molecule".to_string(),
                data_type: DataType::Atomic,
            },
            Parameter {
                id: None,
                name: "tolerance".to_string(),
                data_type: DataType::Float,
            },
        ],
        output_pins: vec![
            OutputPinDefinition {
                name: "result".to_string(),
                data_type: DataType::Atomic,
            },
            OutputPinDefinition {
                name: "diff".to_string(),
                data_type: DataType::Atomic,
            },
        ],
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

pub fn get_node_type_motif_edit() -> NodeType {
    NodeType {
        name: "motif_edit".to_string(),
        description: "Interactive motif editor. Places atoms in Cartesian space; \
            outputs a Motif with fractional coordinates computed from the unit cell. \
            Backed by the same diff-based architecture as atom_edit.\n\
            \n\
            Connect a unit_cell to define the basis vectors for coordinate conversion. \
            Pin 0 (result) outputs a Motif for use with atom_fill. \
            Pin 1 (diff) outputs the raw Atomic diff for inspection."
            .to_string(),
        summary: Some("Visual motif editor".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "molecule".to_string(),
                data_type: DataType::Atomic,
            },
            Parameter {
                id: None,
                name: "unit_cell".to_string(),
                data_type: DataType::UnitCell,
            },
            Parameter {
                id: None,
                name: "tolerance".to_string(),
                data_type: DataType::Float,
            },
        ],
        output_pins: vec![
            OutputPinDefinition {
                name: "result".to_string(),
                data_type: DataType::Motif,
            },
            OutputPinDefinition {
                name: "diff".to_string(),
                data_type: DataType::Atomic,
            },
        ],
        public: true,
        node_data_creator: || Box::new(AtomEditData::new_motif_mode()),
        node_data_saver: |node_data, _design_dir| {
            if let Some(data) = node_data.as_any_mut().downcast_ref::<AtomEditData>() {
                let serializable = atom_edit_data_to_serializable(data)?;
                serde_json::to_value(serializable)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Data type mismatch for motif_edit",
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
// Cartesian → Motif conversion
// =============================================================================

/// Converts an AtomicStructure (Cartesian coordinates) to a Motif (fractional coordinates).
/// Maps parameter element reserved atomic numbers (-100, -101, ...) to motif convention (-1, -2, ...).
/// Uses `cross_cell_bonds` to set `relative_cell` on bonds spanning neighboring cells.
///
/// Public for testing. Normal callers use `eval_motif_mode` which calls this internally.
pub fn atomic_structure_to_motif(
    structure: &AtomicStructure,
    unit_cell: &UnitCellStruct,
    parameter_elements: &[(String, i16)],
    cross_cell_bonds: &HashMap<BondReference, glam::IVec3>,
) -> Motif {
    use super::types::{is_param_element, param_atomic_number_to_motif};
    use crate::crystolecule::motif::ParameterElement;

    // Build parameters list
    let parameters: Vec<ParameterElement> = parameter_elements
        .iter()
        .map(|(name, default_z)| ParameterElement {
            name: name.clone(),
            default_atomic_number: *default_z,
        })
        .collect();

    let mut sites = Vec::new();
    let mut atom_id_to_site_index: HashMap<u32, usize> = HashMap::new();

    for (idx, (_, atom)) in structure.iter_atoms().enumerate() {
        let frac_pos = unit_cell.real_to_dvec3_lattice(&atom.position);

        // Map atomic number: parameter reserved range → motif convention
        let motif_z = if is_param_element(atom.atomic_number) {
            param_atomic_number_to_motif(atom.atomic_number)
        } else {
            atom.atomic_number
        };

        sites.push(Site {
            atomic_number: motif_z,
            position: frac_pos,
        });
        atom_id_to_site_index.insert(atom.id, idx);
    }

    // Convert bonds — look up cross_cell_bonds for relative_cell offsets.
    // Each bond is stored on both atoms; only process where atom_id < other_id.
    let mut bonds = Vec::new();
    for (_, atom) in structure.iter_atoms() {
        for bond in &atom.bonds {
            let other_id = bond.other_atom_id();
            if atom.id < other_id {
                if let (Some(&idx1), Some(&idx2)) = (
                    atom_id_to_site_index.get(&atom.id),
                    atom_id_to_site_index.get(&other_id),
                ) {
                    // Look up cross-cell offset. The stored offset is
                    // "max(id1,id2) relative to min(id1,id2)".
                    // Since we iterate with atom.id < other_id, max=other_id.
                    // site_1 always gets ZERO; site_2 gets the raw offset.
                    let bond_ref = BondReference {
                        atom_id1: atom.id,
                        atom_id2: other_id,
                    };
                    let raw_offset = cross_cell_bonds
                        .get(&bond_ref)
                        .copied()
                        .unwrap_or(glam::IVec3::ZERO);

                    // raw_offset is offset of max(id1,id2)=other_id relative to min=atom.id.
                    // In the motif, site_1 is at ZERO, site_2 gets the offset.
                    bonds.push(MotifBond {
                        site_1: SiteSpecifier {
                            site_index: idx1,
                            relative_cell: glam::IVec3::ZERO,
                        },
                        site_2: SiteSpecifier {
                            site_index: idx2,
                            relative_cell: raw_offset,
                        },
                        multiplicity: bond.bond_order() as i32,
                    });
                }
            }
        }
    }

    // Build precomputed bond index maps
    let site_count = sites.len();
    let mut bonds_by_site1_index = vec![Vec::new(); site_count];
    let mut bonds_by_site2_index = vec![Vec::new(); site_count];
    for (bond_idx, bond) in bonds.iter().enumerate() {
        bonds_by_site1_index[bond.site_1.site_index].push(bond_idx);
        bonds_by_site2_index[bond.site_2.site_index].push(bond_idx);
    }

    Motif {
        parameters,
        sites,
        bonds,
        bonds_by_site1_index,
        bonds_by_site2_index,
    }
}

/// Generates ghost atoms from neighboring unit cells and adds them to the display
/// structure. For each of the 26 neighboring cells, copies atoms whose fractional
/// distance from the primary cell is less than `neighbor_depth`, translates them by
/// the cell offset, and flags them with ATOM_FLAG_GHOST.
///
/// Also generates symmetric bonds for cross-cell bonds: for each cross-cell bond
/// between primary atoms A and B with offset, renders A→ghost_of_B and B→ghost_of_A.
///
/// Ghost atoms are display-only — they are never included in the wire result.
pub fn generate_ghost_atoms(
    viz: &mut AtomicStructure,
    unit_cell: &UnitCellStruct,
    neighbor_depth: f64,
    cross_cell_bonds: &HashMap<BondReference, glam::IVec3>,
) {
    // Collect primary atom data first to avoid borrow conflict.
    // Store (original_id, atomic_number, position, bonds_to_other_primary_ids).
    let primary_atoms: Vec<(u32, i16, DVec3, Vec<(u32, u8)>)> = viz
        .iter_atoms()
        .map(|(_, atom)| {
            let bonds: Vec<(u32, u8)> = atom
                .bonds
                .iter()
                .map(|b| (b.other_atom_id(), b.bond_order()))
                .collect();
            (atom.id, atom.atomic_number, atom.position, bonds)
        })
        .collect();

    // Build original_id → index map for bond resolution
    let id_to_idx: HashMap<u32, usize> = primary_atoms
        .iter()
        .enumerate()
        .map(|(idx, (id, _, _, _))| (*id, idx))
        .collect();

    // Precompute fractional positions for all primary atoms
    let frac_positions: Vec<DVec3> = primary_atoms
        .iter()
        .map(|(_, _, pos, _)| unit_cell.real_to_dvec3_lattice(pos))
        .collect();

    // For each of the 26 neighboring cells
    for dx in -1..=1i32 {
        for dy in -1..=1i32 {
            for dz in -1..=1i32 {
                if dx == 0 && dy == 0 && dz == 0 {
                    continue;
                }

                let cell_offset = DVec3::new(dx as f64, dy as f64, dz as f64);
                let translation = unit_cell.dvec3_lattice_to_real(&cell_offset);

                // Map from primary atom index → ghost atom ID (for bond creation)
                let mut primary_idx_to_ghost: HashMap<usize, u32> = HashMap::new();

                for (atom_idx, frac) in frac_positions.iter().enumerate() {
                    // Fractional position of this atom in the neighboring cell
                    let ghost_frac = *frac + cell_offset;

                    // Compute minimum distance from ghost to nearest face of [0,1]^3
                    let dist = min_distance_to_unit_cube(&ghost_frac);

                    if dist < neighbor_depth {
                        let (primary_id, atomic_number, position, _) = &primary_atoms[atom_idx];
                        let ghost_pos = *position + translation;
                        let ghost_id = viz.add_atom(*atomic_number, ghost_pos);
                        viz.set_atom_ghost(ghost_id, true);
                        // Store ghost metadata for cross-cell bond creation
                        let cell_ivec3 = glam::IVec3::new(dx, dy, dz);
                        viz.decorator_mut()
                            .ghost_atom_metadata
                            .insert(ghost_id, (*primary_id, cell_ivec3));
                        primary_idx_to_ghost.insert(atom_idx, ghost_id);
                    }
                }

                // Create bonds between ghost atoms that both exist in this cell
                for (&atom_idx, &ghost_id) in &primary_idx_to_ghost {
                    let (_, _, _, ref bonds) = primary_atoms[atom_idx];
                    for &(other_primary_id, bond_order) in bonds {
                        if let Some(&other_idx) = id_to_idx.get(&other_primary_id) {
                            if let Some(&other_ghost_id) = primary_idx_to_ghost.get(&other_idx) {
                                // Only add each bond once (lower ghost ID first)
                                if ghost_id < other_ghost_id {
                                    viz.add_bond(ghost_id, other_ghost_id, bond_order);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Generate symmetric cross-cell bonds between primary atoms and ghosts.
    // For each cross-cell bond (A, B, offset), render:
    //   - A (primary) → ghost of B in cell `offset_of_B`
    //   - B (primary) → ghost of A in cell `-offset_of_B`
    // The ghost atoms were already created above; we just need to find them
    // in the ghost_atom_metadata and create the bond segments.
    if !cross_cell_bonds.is_empty() {
        // Build a lookup: (primary_id, cell_offset) → ghost_id
        let ghost_lookup: HashMap<(u32, glam::IVec3), u32> = viz
            .decorator()
            .ghost_atom_metadata
            .iter()
            .map(|(&ghost_id, &(primary_id, offset))| ((primary_id, offset), ghost_id))
            .collect();

        for (bond_ref, &raw_offset) in cross_cell_bonds {
            let atom_a = bond_ref.atom_id1.min(bond_ref.atom_id2);
            let atom_b = bond_ref.atom_id1.max(bond_ref.atom_id2);
            // raw_offset = offset of max(a,b)=atom_b relative to min(a,b)=atom_a

            // Look up bond order from the diff structure
            let bond_order = viz
                .get_atom(atom_a)
                .and_then(|a| {
                    a.bonds
                        .iter()
                        .find(|b| b.other_atom_id() == atom_b)
                        .map(|b| b.bond_order())
                })
                .unwrap_or(1);

            // Direction A→B: B is in cell raw_offset relative to A
            // Render bond from primary A to ghost of B in cell raw_offset
            if let Some(&ghost_b) = ghost_lookup.get(&(atom_b, raw_offset)) {
                viz.add_bond(atom_a, ghost_b, bond_order);
            }

            // Direction B→A: A is in cell -raw_offset relative to B
            // Render bond from primary B to ghost of A in cell -raw_offset
            if let Some(&ghost_a) = ghost_lookup.get(&(atom_a, -raw_offset)) {
                viz.add_bond(atom_b, ghost_a, bond_order);
            }
        }
    }
}

/// Computes the fractional distance from a ghost atom to the nearest face of
/// the primary cell [0,1]^3.
///
/// For an atom outside the cell (ghost), this is the perpendicular distance
/// from the cell boundary along the axis(es) that were crossed. When multiple
/// axes are crossed (corner/edge ghosts), returns the maximum per-axis distance
/// — the atom must be within `neighbor_depth` on every crossed axis.
///
/// For an atom inside the cell, returns the distance to the nearest face.
pub fn min_distance_to_unit_cube(frac: &DVec3) -> f64 {
    let dist_x = if frac.x < 0.0 {
        -frac.x
    } else if frac.x > 1.0 {
        frac.x - 1.0
    } else {
        0.0
    };
    let dist_y = if frac.y < 0.0 {
        -frac.y
    } else if frac.y > 1.0 {
        frac.y - 1.0
    } else {
        0.0
    };
    let dist_z = if frac.z < 0.0 {
        -frac.z
    } else if frac.z > 1.0 {
        frac.z - 1.0
    } else {
        0.0
    };

    let outside = dist_x > 0.0 || dist_y > 0.0 || dist_z > 0.0;
    if outside {
        // Ghost atom outside the cell: max per-axis overshoot
        dist_x.max(dist_y).max(dist_z)
    } else {
        // Inside the cell: min distance to nearest face
        let dx = frac.x.min(1.0 - frac.x);
        let dy = frac.y.min(1.0 - frac.y);
        let dz = frac.z.min(1.0 - frac.z);
        dx.min(dy).min(dz)
    }
}

// =============================================================================
// Helper accessors
// =============================================================================

/// Check if a node type name belongs to the atom_edit family
/// (atom_edit or motif_edit — both backed by AtomEditData).
pub fn is_atom_edit_family(name: &str) -> bool {
    name == "atom_edit" || name == "motif_edit"
}

/// Get the selected node ID for any atom_edit family node (atom_edit or motif_edit).
pub(crate) fn get_selected_atom_edit_family_node_id(
    structure_designer: &StructureDesigner,
) -> Option<u64> {
    structure_designer
        .get_selected_node_id_with_type("atom_edit")
        .or_else(|| structure_designer.get_selected_node_id_with_type("motif_edit"))
}

/// Gets the AtomEditData for the currently active atom_edit/motif_edit node (immutable)
pub fn get_active_atom_edit_data(structure_designer: &StructureDesigner) -> Option<&AtomEditData> {
    let selected_node_id = get_selected_atom_edit_family_node_id(structure_designer)?;
    let node_data = structure_designer.get_node_network_data(selected_node_id)?;
    node_data.as_any_ref().downcast_ref::<AtomEditData>()
}

/// Gets mutable access to AtomEditData WITHOUT marking the node data as changed.
/// Use for transient state changes (interaction_state) that don't affect evaluation.
pub(super) fn get_atom_edit_data_mut_transient(
    structure_designer: &mut StructureDesigner,
) -> Option<&mut AtomEditData> {
    let selected_node_id = get_selected_atom_edit_family_node_id(structure_designer)?;
    let node_data = structure_designer.get_node_network_data_mut(selected_node_id)?;
    node_data.as_any_mut().downcast_mut::<AtomEditData>()
}

/// Gets the AtomEditData for the currently selected atom_edit node (mutable)
///
/// Automatically marks the node data as changed since this is only called for mutations.
pub fn get_selected_atom_edit_data_mut(
    structure_designer: &mut StructureDesigner,
) -> Option<&mut AtomEditData> {
    let selected_node_id = get_selected_atom_edit_family_node_id(structure_designer)?;
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

// =============================================================================
// Undo/redo recording helpers
// =============================================================================

/// Get the network_name and node_id for the currently selected atom_edit node.
/// Public variant for use by API layer (toggle flag / frozen change commands).
pub fn get_atom_edit_node_info_pub(
    structure_designer: &StructureDesigner,
) -> Option<(String, u64)> {
    get_atom_edit_node_info(structure_designer)
}

/// Get the network_name and node_id for the currently selected atom_edit/motif_edit node.
fn get_atom_edit_node_info(structure_designer: &StructureDesigner) -> Option<(String, u64)> {
    let network_name = structure_designer
        .active_node_network_name
        .as_ref()?
        .clone();
    let node_id = get_selected_atom_edit_family_node_id(structure_designer)?;
    Some((network_name, node_id))
}

/// Get mutable access to AtomEditData for recording setup/teardown.
/// Does NOT call mark_node_data_changed — recording setup is not a data mutation.
fn get_atom_edit_data_for_recording(
    structure_designer: &mut StructureDesigner,
) -> Option<&mut AtomEditData> {
    let network_name = structure_designer.active_node_network_name.as_ref()?;
    let selected_node_id = get_selected_atom_edit_family_node_id(structure_designer)?;
    let network = structure_designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)?;
    let node_data = network.get_node_network_data_mut(selected_node_id)?;
    node_data.as_any_mut().downcast_mut::<AtomEditData>()
}

/// Temporary state held during an atom edit drag (screen-plane or gadget).
/// Recording is active on AtomEditData.recorder throughout the drag.
pub struct PendingAtomEditDrag {
    pub network_name: String,
    pub node_id: u64,
    /// Tracks base atoms promoted to diff by continuous minimization.
    /// Maps base atom ID → diff atom ID. Created empty at drag start,
    /// populated during write-back as BasePassthrough atoms are promoted,
    /// and dropped when the drag ends.
    pub promoted_base_atoms: HashMap<u32, u32>,
}

/// Begin recording for a drag operation on the atom_edit node.
///
/// Starts recording on AtomEditData and stores a `PendingAtomEditDrag` on
/// StructureDesigner. All subsequent mutations (drag_selected_by_delta, gadget
/// sync_data) accumulate deltas in the recorder. Call `end_atom_edit_drag()`
/// when the drag completes to push a single undo command.
pub fn begin_atom_edit_drag(structure_designer: &mut StructureDesigner) {
    let (network_name, node_id) = match get_atom_edit_node_info(structure_designer) {
        Some(info) => info,
        None => return,
    };

    if let Some(data) = get_atom_edit_data_for_recording(structure_designer) {
        data.begin_recording();
    }

    structure_designer.pending_atom_edit_drag = Some(PendingAtomEditDrag {
        network_name,
        node_id,
        promoted_base_atoms: HashMap::new(),
    });
}

/// End recording for a drag operation and push the undo command.
///
/// Takes the accumulated deltas from the recorder, coalesces them, and pushes
/// a single `AtomEditMutationCommand`. If no deltas were produced (e.g., drag
/// without movement), no command is pushed.
pub fn end_atom_edit_drag(structure_designer: &mut StructureDesigner) {
    let pending = match structure_designer.pending_atom_edit_drag.take() {
        Some(p) => p,
        None => return,
    };

    if let Some(data) = get_atom_edit_data_for_recording(structure_designer) {
        if let Some(recorder) = data.end_recording() {
            if !recorder.atom_deltas.is_empty()
                || !recorder.bond_deltas.is_empty()
                || !recorder.cross_cell_bond_deltas.is_empty()
            {
                structure_designer.push_command(
                    crate::structure_designer::undo::commands::atom_edit_mutation::AtomEditMutationCommand {
                        description: "Move atoms".to_string(),
                        network_name: pending.network_name,
                        node_id: pending.node_id,
                        atom_deltas: recorder.atom_deltas,
                        bond_deltas: recorder.bond_deltas,
                        cross_cell_bond_deltas: recorder.cross_cell_bond_deltas,
                    },
                );
            }
        }
    }
}

/// Wrap a mutation on `StructureDesigner` with atom_edit undo recording.
///
/// 1. Begins recording on the atom_edit data.
/// 2. Executes the mutation closure.
/// 3. Ends recording and pushes an `AtomEditMutationCommand` if deltas were produced.
///
/// If no atom_edit node is selected, the mutation runs without recording.
pub fn with_atom_edit_undo<F>(
    structure_designer: &mut StructureDesigner,
    description: &str,
    mutation: F,
) where
    F: FnOnce(&mut StructureDesigner),
{
    let (network_name, node_id) = match get_atom_edit_node_info(structure_designer) {
        Some(info) => info,
        None => {
            mutation(structure_designer);
            return;
        }
    };

    // Begin recording
    if let Some(data) = get_atom_edit_data_for_recording(structure_designer) {
        data.begin_recording();
    }

    // Execute the mutation
    mutation(structure_designer);

    // End recording and push command
    if let Some(data) = get_atom_edit_data_for_recording(structure_designer) {
        if let Some(recorder) = data.end_recording() {
            if !recorder.atom_deltas.is_empty()
                || !recorder.bond_deltas.is_empty()
                || !recorder.cross_cell_bond_deltas.is_empty()
            {
                structure_designer.push_command(
                    crate::structure_designer::undo::commands::atom_edit_mutation::AtomEditMutationCommand {
                        description: description.to_string(),
                        network_name,
                        node_id,
                        atom_deltas: recorder.atom_deltas,
                        bond_deltas: recorder.bond_deltas,
                        cross_cell_bond_deltas: recorder.cross_cell_bond_deltas,
                    },
                );
            }
        }
    }
}
