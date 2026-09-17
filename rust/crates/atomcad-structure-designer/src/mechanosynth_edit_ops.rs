//! Everything a `mechanosynth_edit` node's panel and viewport tool ask the
//! designer to do: the four block edits, the cursor, and the placement flow.
//!
//! It lives here rather than in `structure_designer.rs` for the reason
//! `ai_text_edit.rs` does — it is ordinary domain work (read the workpiece,
//! call the engine, edit the block, record an undo entry) reachable from a
//! plain `StructureDesigner::new()`, and the api layer above it is four-line
//! wrappers. See `rust/AGENTS.md` §Testing.
//!
//! Three rules the whole module is arranged around:
//!
//! - **The cursor is not an undo entry.** Every block edit goes through
//!   [`StructureDesigner::edit_mechanosynth_block`], which records the
//!   `(authored, cursor)` pair; the cursor setter mutates in place and records
//!   nothing. Scrubbing is navigation, exactly as the replayer's slider is.
//! - **Nothing here goes through `set_node_network_data_scoped`**, which would
//!   push a `SetNodeDataCommand` of its own and give a placement two undo
//!   entries (or give a scrub one). In-place mutation through
//!   `get_node_network_data_mut_scoped` is what keeps the count right.
//! - **The workpiece is evaluated on demand, never cached.** A pick asks for
//!   the node's own `result` pin, which is the state the user is looking at by
//!   construction. A cache would have to be invalidated by every upstream edit,
//!   and the sweep is a per-click cost the design already budgets for.

use crate::nodes::build_step::steps_from_array;
use crate::nodes::mechanosynth_edit::{
    AuthoredStep, MechanosynthEditData, OPS_PIN, SCENE_OUTPUT_PIN, STEPS_PIN, ToolState,
};
use crate::structure_designer::StructureDesigner;
use crate::undo::commands::mechanosynth_edit_block::{
    MechanosynthEditBlockCommand, MetadataEditKey,
};
use crate::undo::commands::mechanosynth_edit_mute::MechanosynthEditMuteCommand;
use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::mechanosynth::{
    Applicability, Candidate, GhostAtom, OpLibrary, Operation, Participant, Scene, Step,
    ToolReadiness, applicable_ops_where, preview_atoms, preview_bonds, resolve_tolerance,
    steps_applied,
};
use glam::DVec3;
use std::collections::BTreeSet;

/// The open metadata-edit run: which chip is being typed into, where its first
/// command landed on the undo stack, and the block state that command reverts
/// to. Lives on [`StructureDesigner`] and is cleared by anything that is not
/// the next keystroke of the same run.
#[derive(Debug, Clone)]
pub struct PendingStepMetadataEdit {
    pub key: MetadataEditKey,
    /// [`UndoStack::push_count`](crate::undo::UndoStack::push_count) right after
    /// the run's most recent command. A mismatch means something else was
    /// pushed in between, so the run is over.
    pub push_count: u64,
    pub before: (Vec<AuthoredStep>, i32),
}

/// One row of the offer popup: what an operation says about the clicked atom,
/// plus the ghost atoms that preview it.
///
/// The ghosts are built **here**, where the workpiece is already in hand, so
/// highlighting a row redraws a preview without a second `place` call and
/// without the whole structure crossing a boundary. A near-miss row's ghosts
/// come from its `near_miss` candidate: it is for showing, never for placing.
#[derive(Debug, Clone, PartialEq)]
pub struct OfferRow {
    pub op: String,
    /// The library's own note for the operation; empty when it has none.
    pub note: String,
    pub candidate_count: usize,
    pub best_residual: f64,
    /// Whether this row is applicable at all. `false` makes it a near miss:
    /// shown with its residual, and refused by `choose`.
    pub fits: bool,
    pub exact: bool,
    pub mirrored: bool,
    pub approximate: bool,
    pub ghost: Vec<GhostAtom>,
    /// **Every** way of placing this operation here, each with its own ghosts.
    ///
    /// The popup lists them inline, under the operation's name, so choosing an
    /// orientation is one click rather than a click into a second list. A near
    /// miss carries its one rejected fit here too, so it previews like
    /// anything else.
    pub candidates: Vec<CandidateRow>,
    /// What this row's tool has to say, when tools are wired and the operation
    /// is `tip`. `None` otherwise — with `tools` unwired no row carries a tool
    /// annotation, which is the modelling use of the editor exactly as it was
    /// before tools existed.
    pub tool: Option<ToolReadiness>,
    /// Whether the row can be committed at all: it fits **and** its tool is
    /// ready. A row that fits but whose tool is not sits below the rule with
    /// the near misses, dimmed, with the reason where the residual would be.
    pub offerable: bool,
    /// The node mutes this operation, so only a *show all here* sweep can have
    /// produced the row. The popup badges it, to say why it was not there a
    /// moment ago.
    ///
    /// **Not a reason to refuse it.** Mute filters the sweep; a row that is in
    /// the list is placeable, whatever put it there.
    pub muted: bool,
    /// Why **every** way of placing this operation here is refused, in the
    /// words the row shows where its residual would be.
    ///
    /// `None` for a row with at least one placeable candidate — a *mixed* row
    /// stays above the rule and dims only the candidates that are refused
    /// (`doc/design_mechanosynth_pattern_checks.md` §5.2). Set, it puts the row
    /// below the rule the way a near miss goes there, and `choose` refuses it
    /// at the row before it looks at the index.
    pub blocked: Option<String>,
}

/// One row of the candidate list: which way of placing the chosen operation,
/// and what it would leave behind.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateRow {
    /// Index into the operation's candidate list — what `choose` takes.
    pub index: usize,
    pub residual: f64,
    pub exact: bool,
    pub mirrored: bool,
    pub approximate: bool,
    pub ghost: Vec<GhostAtom>,
    /// Why this particular placement cannot be committed, when it cannot: the
    /// clicked atom's degree, or the atom this way of placing it would land on.
    ///
    /// A refused candidate is **kept, at its index**, because the reason is
    /// about the ghost the user is looking at — so it previews like any other
    /// and `choose` refuses it by index.
    pub blocked: Option<String>,
}

/// An applicability sweep, with everything the popup needs to place and write
/// itself: the anchor atom it hangs off, and one row per operation with
/// anything to say.
#[derive(Debug, Clone, PartialEq)]
pub struct OfferSweep {
    pub anchor_atom_id: u32,
    pub anchor_position: DVec3,
    /// The anchor's atomic number — the popup header says "3 operations apply
    /// to this Si".
    pub anchor_atomic_number: i16,
    pub rows: Vec<OfferRow>,
    /// How many of the wired library's operations the sweep **did not look
    /// at**, because the node mutes them.
    ///
    /// Always reported, whether or not any of them would have fitted: the offer
    /// list's promise is that an empty result is a statement about the library
    /// (`doc/design_mechanosynth_editor.md` §*A miss becomes a coverage
    /// report*), and a filter that said nothing would turn that into a lie.
    /// Deliberately **not** "how many muted ops apply here" — knowing that
    /// costs exactly the sweep the mute avoids.
    pub skipped_muted: usize,
    /// How many operations the wired library defines, so the popup's line can
    /// say *4 of 19* rather than a bare *4*. A proportion is what makes the
    /// number mean something at a glance.
    pub library_count: usize,
}

/// The atom a click landed on: what the popup hangs off and heads itself with.
/// Carried separately from [`OfferSweep`] because the candidate-list path has no
/// sweep of its own and still has to be anchored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnchorInfo {
    pub atom_id: u32,
    pub position: DVec3,
    pub atomic_number: i16,
}

/// Builds the popup's view of a sweep. `workpiece` and `library` are the ones
/// the sweep ran against, so the ghosts and the residuals describe the same
/// state.
fn offer_sweep(
    workpiece: &AtomicStructure,
    library: &OpLibrary,
    atom_id: u32,
    rows: &[Applicability],
    skipped_muted: usize,
    muted: &BTreeSet<String>,
) -> OfferSweep {
    let anchor = workpiece
        .get_atom(atom_id)
        .expect("the clicked atom exists");
    OfferSweep {
        anchor_atom_id: atom_id,
        anchor_position: anchor.position,
        anchor_atomic_number: anchor.atomic_number,
        skipped_muted,
        library_count: library.ops.len(),
        rows: rows
            .iter()
            .map(|row| {
                let preview = row.preview();
                let operation = library.get(&row.op);
                let candidates = (0..row.candidates.len().max(1))
                    .filter_map(|index| {
                        let candidate = row.preview_at(index)?;
                        Some(CandidateRow {
                            index,
                            residual: candidate.residual,
                            exact: candidate.exact,
                            mirrored: candidate.mirrored,
                            approximate: candidate.approximate,
                            ghost: operation
                                .map(|op| preview_atoms(workpiece, op, candidate))
                                .unwrap_or_default(),
                            blocked: candidate.refusal.map(|refusal| refusal.reason(&row.op)),
                        })
                    })
                    .collect();
                OfferRow {
                    op: row.op.clone(),
                    note: library
                        .get(&row.op)
                        .and_then(|op| op.note.clone())
                        .unwrap_or_default(),
                    candidate_count: row.candidates.len(),
                    best_residual: row.best_residual,
                    fits: row.fits,
                    exact: preview.exact,
                    mirrored: preview.mirrored,
                    approximate: row.approximate,
                    ghost: operation
                        .map(|op| preview_atoms(workpiece, op, preview))
                        .unwrap_or_default(),
                    candidates,
                    tool: row.tool.clone(),
                    offerable: row.offerable(),
                    muted: muted.contains(&row.op),
                    blocked: row.blocked(),
                }
            })
            .collect(),
    }
}

/// Which metadata field a chip edit writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepMetadataField {
    Note,
    Phase,
    Layer,
    Site,
}

impl StepMetadataField {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "note" => Some(Self::Note),
            "phase" => Some(Self::Phase),
            "layer" => Some(Self::Layer),
            "site" => Some(Self::Site),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Phase => "phase",
            Self::Layer => "layer",
            Self::Site => "site",
        }
    }
}

impl StructureDesigner {
    // ========================================================================
    // Reading
    // ========================================================================

    /// The stored data of a `mechanosynth_edit` node, or `None` if the address
    /// names something else.
    pub fn mechanosynth_edit_data(
        &self,
        scope_path: &[u64],
        node_id: u64,
    ) -> Option<&MechanosynthEditData> {
        self.get_node_network_data_scoped(scope_path, node_id)?
            .as_any_ref()
            .downcast_ref::<MechanosynthEditData>()
    }

    fn mechanosynth_edit_data_mut(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
    ) -> Option<&mut MechanosynthEditData> {
        self.get_node_network_data_mut_scoped(scope_path, node_id)?
            .as_any_mut()
            .downcast_mut::<MechanosynthEditData>()
    }

    /// The **scene** the tool acts on: the state at the cursor, with the
    /// reservoirs and the tool molecules in it, which is what the user is
    /// looking at (`scene` is the display default).
    ///
    /// Evaluating the node is what produces it — the scene carries the tool
    /// bindings and the participant map, neither of which survives the trip
    /// through a pin, so the node parks it in its own data and this reads it
    /// back. An *errored* pin is not a failure here: when the block fails at
    /// the cursor step the stored scene is the state after the last successful
    /// step, which is exactly what the viewport is showing and therefore what a
    /// click must be resolved against.
    fn mechanosynth_edit_scene(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
    ) -> Result<Scene, String> {
        let evaluated = self.evaluate_node_output(scope_path, node_id, SCENE_OUTPUT_PIN as i32);
        self.mechanosynth_edit_data(scope_path, node_id)
            .ok_or("Not a mechanosynth_edit node")?
            .last_scene()
            .ok_or_else(|| match evaluated {
                crate::evaluator::network_result::NetworkResult::Error(message) => message,
                _ => "the node produced no atoms".to_string(),
            })
    }

    /// The wired operation library.
    fn mechanosynth_edit_library(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
    ) -> Result<std::sync::Arc<OpLibrary>, String> {
        match self.evaluate_node_argument(scope_path, node_id, OPS_PIN) {
            crate::evaluator::network_result::NetworkResult::OpLibrary(library) => Ok(library),
            crate::evaluator::network_result::NetworkResult::Error(message) => Err(message),
            _ => Err("no operation library (wire the ops pin)".to_string()),
        }
    }

    /// The steps arriving on the `steps` pin — the block's prefix. An empty
    /// list when nothing is wired or the upstream failed: a metadata template
    /// is not worth failing a placement over.
    fn mechanosynth_edit_prefix(&mut self, scope_path: &[u64], node_id: u64) -> Vec<Step> {
        match self.evaluate_node_argument(scope_path, node_id, STEPS_PIN) {
            crate::evaluator::network_result::NetworkResult::None => Vec::new(),
            crate::evaluator::network_result::NetworkResult::Error(_) => Vec::new(),
            array => steps_from_array(&array).unwrap_or_default(),
        }
    }

    // ========================================================================
    // Block edits
    // ========================================================================

    /// The one path every block edit takes: read the `(authored, cursor)` pair,
    /// let `mutate` change it, write it back in place, and record one undo
    /// entry.
    ///
    /// `coalesce` is `Some` only for a metadata edit; two consecutive edits
    /// sharing a key merge into the entry the first one pushed, so typing into
    /// a chip costs one undo step rather than one per keystroke.
    fn edit_mechanosynth_block(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        description: String,
        coalesce: Option<MetadataEditKey>,
        mutate: impl FnOnce(&mut Vec<AuthoredStep>, &mut i32) -> Result<(), String>,
    ) -> Result<(), String> {
        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or("No active network")?;

        let before = {
            let data = self
                .mechanosynth_edit_data(scope_path, node_id)
                .ok_or("Not a mechanosynth_edit node")?;
            (data.authored.clone(), data.cursor)
        };

        let mut authored = before.0.clone();
        let mut cursor = before.1;
        mutate(&mut authored, &mut cursor)?;
        let after = (authored, cursor);
        if after == before {
            return Ok(());
        }

        {
            let data = self
                .mechanosynth_edit_data_mut(scope_path, node_id)
                .ok_or("Not a mechanosynth_edit node")?;
            data.authored = after.0.clone();
            data.cursor = after.1;
        }
        self.set_dirty(true);

        // Coalescing: the previous command is ours, is still the top of the
        // stack (the push count says nothing has happened since), and covers the
        // same field of the same step — so it is replaced by one spanning the
        // whole run. The run's *original* `before` comes from the pending
        // record, not from this edit, which is what makes one undo revert the
        // whole typed word rather than its last letter.
        let pending = self.pending_step_metadata_edit.take();
        let effective_before = match (&coalesce, pending) {
            (Some(key), Some(pending))
                if pending.key == *key && pending.push_count == self.undo_stack.push_count() =>
            {
                self.undo_stack.pop_last();
                pending.before
            }
            _ => before,
        };

        self.push_command(MechanosynthEditBlockCommand {
            network_name,
            scope_path: scope_path.to_vec(),
            node_id,
            description,
            before: effective_before.clone(),
            after,
            coalesce: coalesce.clone(),
        });
        if let Some(key) = coalesce {
            self.pending_step_metadata_edit = Some(PendingStepMetadataEdit {
                key,
                push_count: self.undo_stack.push_count(),
                before: effective_before,
            });
        }
        Ok(())
    }

    /// Inserts `step` at `index` and moves the cursor onto it.
    pub fn mechanosynth_edit_insert_step(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        index: usize,
        step: AuthoredStep,
    ) -> Result<(), String> {
        self.edit_mechanosynth_block(
            scope_path,
            node_id,
            format!("Insert {} step", step.step.op),
            None,
            move |authored, cursor| {
                if index > authored.len() {
                    return Err(format!(
                        "step index {index} is past the end of the block ({})",
                        authored.len()
                    ));
                }
                authored.insert(index, step);
                // The cursor lands *on* the new step, so the viewport shows the
                // reaction that has just been placed.
                *cursor = index as i32 + 1;
                Ok(())
            },
        )
    }

    /// Deletes the step at `index`. The cursor follows it back by one, so the
    /// viewport keeps showing the state the deletion left behind.
    pub fn mechanosynth_edit_delete_step(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        index: usize,
    ) -> Result<(), String> {
        self.edit_mechanosynth_block(
            scope_path,
            node_id,
            "Delete build step".to_string(),
            None,
            move |authored, cursor| {
                if index >= authored.len() {
                    return Err(format!("no step at index {index}"));
                }
                let applied = steps_applied(*cursor, authored.len());
                authored.remove(index);
                // A cursor that was at or past the deleted step steps back with
                // it; `-1` ("all") keeps following the end.
                if *cursor >= 0 && applied > index {
                    *cursor = applied as i32 - 1;
                }
                Ok(())
            },
        )
    }

    /// Moves the step at `from` to `to`, carrying its residual and flag.
    pub fn mechanosynth_edit_move_step(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        from: usize,
        to: usize,
    ) -> Result<(), String> {
        self.edit_mechanosynth_block(
            scope_path,
            node_id,
            "Reorder build step".to_string(),
            None,
            move |authored, _cursor| {
                if from >= authored.len() {
                    return Err(format!("no step at index {from}"));
                }
                if to >= authored.len() {
                    return Err(format!("cannot move a step to index {to}"));
                }
                let step = authored.remove(from);
                authored.insert(to, step);
                Ok(())
            },
        )
    }

    // ========================================================================
    // One-shot imports
    // ========================================================================
    //
    // Both of these copy steps *into* the block, where they become ordinary
    // authored steps with no memory of where they came from. That is the whole
    // difference from the `steps` pin and from `build_script`'s `file`
    // property, which stay live: a path that is still on screen is a promise
    // that the node tracks the file, and neither of these keeps one.
    //
    // An imported step is **exact**. A generated file's step is its author's
    // assertion about where the reaction goes, and the editor has no better
    // evidence to offer than the generator had — so the residual is zero and
    // the approximate flag is clear, exactly as for a step typed by hand
    // (`AuthoredStep::residual`).

    /// **Adopt these into the block**: copies the steps arriving on the `steps`
    /// pin to the front of the authored block and disconnects the pin.
    ///
    /// The prefix runs *before* the block, so prepending preserves the replay
    /// order exactly; disconnecting is what keeps the count right, and is why
    /// this cannot be an `edit_mechanosynth_block` edit — that command restores
    /// the block alone and would leave an undo that re-adopted the steps
    /// without putting the wire back. The upstream node is left alone: it may
    /// feed something else, and deleting a node nobody asked to have deleted is
    /// the mistake `convert_mechanosynth_files_to_nodes` avoids too.
    ///
    /// Returns how many steps were adopted.
    pub fn mechanosynth_edit_adopt_prefix(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
    ) -> Result<usize, String> {
        use crate::undo::commands::mechanosynth_edit_adopt::MechanosynthEditAdoptCommand;

        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or("No active network")?;

        // Read the prefix through the same path the engine does, so an adoption
        // copies exactly the steps the block was replaying a moment ago.
        let prefix = self.mechanosynth_edit_prefix(scope_path, node_id);
        if prefix.is_empty() {
            return Err("nothing on the steps pin to adopt".to_string());
        }
        let adopted = prefix.len();

        // Snapshot before the surgery, exactly as `convert_files_to_nodes` does.
        let (top_before, body_before) = if scope_path.is_empty() {
            (self.snapshot_network(&network_name), None)
        } else {
            (None, self.snapshot_zone_body(scope_path))
        };
        self.undo_stack.suppress_recording();

        {
            let data = self
                .mechanosynth_edit_data_mut(scope_path, node_id)
                .ok_or("Not a mechanosynth_edit node")?;
            let mut block: Vec<AuthoredStep> = prefix
                .into_iter()
                .map(|step| AuthoredStep {
                    step,
                    residual: 0.0,
                    approximate: false,
                })
                .collect();
            block.append(&mut data.authored);
            data.authored = block;
            // The cursor counts authored steps, so it has to move with them for
            // the viewport to show what it showed before the press. `-1` means
            // "all" and already does.
            if data.cursor >= 0 {
                data.cursor += adopted as i32;
            }
        }

        // The wire has to go, or the adopted steps replay twice.
        if let Some(network) = self.get_scope_network_mut(scope_path)
            && let Some(node) = network.nodes.get_mut(&node_id)
            && let Some(argument) = node.arguments.get_mut(STEPS_PIN)
        {
            argument.incoming_wires.clear();
        }

        self.undo_stack.resume_recording();

        // The node caches its resolved inputs, the prefix among them
        // (`CachedInputs`), and the refresh system only clears that cache on
        // *displayed* nodes. The wire this just removed is an input of exactly
        // this node, so it drops its own cache rather than trusting a refresh
        // to notice: a stale prefix would replay the adopted steps twice.
        if let Some(data) = self.mechanosynth_edit_data(scope_path, node_id) {
            data.invalidate_input_cache();
        }

        // Refresh paths do not validate, so a stale error would linger.
        self.validate_active_network();

        if scope_path.is_empty() {
            if let (Some(before), Some(after)) = (top_before, self.snapshot_network(&network_name))
            {
                self.push_command(MechanosynthEditAdoptCommand {
                    network_name,
                    before_snapshot: before,
                    after_snapshot: after,
                });
            }
        } else {
            self.push_zone_body_command(
                scope_path,
                "Adopt steps into the block".to_string(),
                body_before,
            );
        }

        self.set_dirty(true);
        self.mark_full_refresh();

        Ok(adopted)
    }

    /// *Insert steps from file…*: splices a build file's steps into the block
    /// at `index`, as one undo entry. The second entry point for the same
    /// one-shot import, for a node with nothing on its `steps` pin.
    ///
    /// No graph changes, so this is an ordinary block edit. `design_dir`
    /// resolves a relative path the way every other file node resolves one —
    /// but nothing about the path is stored afterwards.
    ///
    /// Returns how many steps were inserted.
    pub fn mechanosynth_edit_insert_steps_from_file(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        file: &str,
        index: usize,
        design_dir: Option<&str>,
    ) -> Result<usize, String> {
        let script = crate::nodes::build_script::load_script_at(file, design_dir)?;
        if script.steps.is_empty() {
            return Err(format!("{file} has no steps"));
        }
        let inserted = script.steps.len();
        let steps: Vec<AuthoredStep> = script
            .steps
            .into_iter()
            .map(|step| AuthoredStep {
                step,
                residual: 0.0,
                approximate: false,
            })
            .collect();
        let description = if inserted == 1 {
            "Insert 1 build step from file".to_string()
        } else {
            format!("Insert {inserted} build steps from file")
        };
        self.edit_mechanosynth_block(
            scope_path,
            node_id,
            description,
            None,
            move |authored, cursor| {
                if index > authored.len() {
                    return Err(format!(
                        "step index {index} is past the end of the block ({})",
                        authored.len()
                    ));
                }
                authored.splice(index..index, steps);
                // The cursor lands on the last inserted step, so the viewport
                // shows what the import built rather than the state before it.
                *cursor = (index + inserted) as i32;
                Ok(())
            },
        )?;
        Ok(inserted)
    }

    /// Writes one metadata field of one step. Consecutive writes to the same
    /// field of the same step are one undo entry.
    pub fn set_mechanosynth_edit_step_metadata(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        index: usize,
        field: StepMetadataField,
        text: &str,
        number: i32,
    ) -> Result<(), String> {
        let key = MetadataEditKey {
            node_id,
            scope_path: scope_path.to_vec(),
            step_index: index,
            field: field.as_str().to_string(),
        };
        let text = text.to_string();
        self.edit_mechanosynth_block(
            scope_path,
            node_id,
            format!("Set step {}", field.as_str()),
            Some(key),
            move |authored, _cursor| {
                let authored_step = authored
                    .get_mut(index)
                    .ok_or_else(|| format!("no step at index {index}"))?;
                match field {
                    StepMetadataField::Note => {
                        authored_step.step.note = (!text.is_empty()).then_some(text)
                    }
                    StepMetadataField::Phase => authored_step.step.phase = text,
                    StepMetadataField::Layer => authored_step.step.layer = number,
                    StepMetadataField::Site => authored_step.step.site = number,
                }
                Ok(())
            },
        )
    }

    /// Moves the cursor. **Not** an undo entry, by design.
    pub fn set_mechanosynth_edit_cursor(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        cursor: i32,
    ) -> Result<(), String> {
        let data = self
            .mechanosynth_edit_data_mut(scope_path, node_id)
            .ok_or("Not a mechanosynth_edit node")?;
        if data.cursor == cursor {
            return Ok(());
        }
        data.cursor = cursor;
        // A move invalidates the pending candidates: they were fitted against a
        // workpiece the new cursor no longer shows.
        data.placement.reset();
        self.set_dirty(true);
        // A scrub must not merge into the next chip edit.
        self.pending_step_metadata_edit = None;
        Ok(())
    }

    // ========================================================================
    // Muting
    // ========================================================================

    /// Adds `ops` to the node's mute set, or removes them, in **one** undo
    /// entry however many names it carries.
    ///
    /// Bulk is the primary case, not a convenience: a group chip in the panel
    /// mutes a dozen operations in one user action, and twelve single calls
    /// would be twelve undo entries and twelve refreshes.
    ///
    /// A name the wired library does not define is stored anyway — the `ops`
    /// pin may be rewired, and a mute that evaporated when its library was
    /// briefly swapped would be worse than one that waits.
    ///
    /// **An open offer list is left alone**, deliberately. Muting changes no
    /// fit: the rows were computed against a workpiece this does not touch, so
    /// they are exactly as valid afterwards as before, and the popup hides the
    /// muted one in place rather than paying for a second sweep. Resetting
    /// would also be visible as a bug rather than as tidiness — the viewport
    /// closes the popup whenever the kernel stops saying `offers`, so muting a
    /// row from the popup would shut the list the user was reading.
    pub fn set_mechanosynth_edit_muted(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        ops: &[String],
        muted: bool,
    ) -> Result<(), String> {
        let network_name = self
            .active_node_network_name
            .clone()
            .ok_or("No active network")?;

        let before = self
            .mechanosynth_edit_data(scope_path, node_id)
            .ok_or("Not a mechanosynth_edit node")?
            .muted
            .clone();

        let mut after = before.clone();
        for op in ops {
            if muted {
                after.insert(op.clone());
            } else {
                after.remove(op);
            }
        }
        if after == before {
            return Ok(());
        }

        let description = match (muted, ops) {
            (true, [one]) => format!("Mute {one}"),
            (false, [one]) => format!("Unmute {one}"),
            (true, many) => format!("Mute {} operations", many.len()),
            (false, many) => format!("Unmute {} operations", many.len()),
        };

        {
            let data = self
                .mechanosynth_edit_data_mut(scope_path, node_id)
                .ok_or("Not a mechanosynth_edit node")?;
            data.muted = after.clone();
        }
        self.set_dirty(true);
        // Not a block edit, so it ends any open metadata-typing run rather than
        // merging into it.
        self.pending_step_metadata_edit = None;

        self.push_command(MechanosynthEditMuteCommand {
            network_name,
            scope_path: scope_path.to_vec(),
            node_id,
            description,
            before,
            after,
        });
        Ok(())
    }

    // ========================================================================
    // The placement tool
    // ========================================================================

    /// The atom-first entry point: what the wired library can do at `atom_id`,
    /// minus whatever the node mutes. Never an error for "nothing applies" — an
    /// empty list is an answer.
    pub fn mechanosynth_edit_offers(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        atom_id: u32,
    ) -> Result<OfferSweep, String> {
        self.offer_sweep_at(scope_path, node_id, atom_id, false)
    }

    /// [`Self::mechanosynth_edit_offers`] over the **whole** wired library,
    /// ignoring the node's mute set — the popup's *show all here*.
    ///
    /// The escape hatch that keeps an empty offer list honest once muting
    /// exists: the list's promise is that it reports the library's coverage,
    /// and this is how a user cashes that promise in. Its result replaces the
    /// stored offers, so a row it brings back is fully placeable — mute filters
    /// the sweep and nothing else.
    pub fn mechanosynth_edit_offers_including_muted(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        atom_id: u32,
    ) -> Result<OfferSweep, String> {
        self.offer_sweep_at(scope_path, node_id, atom_id, true)
    }

    /// The sweep both entry points are.
    ///
    /// A named pair rather than one public `include_muted: bool` because "the
    /// ordinary sweep" is what almost every caller means, and a bare `false` at
    /// thirty call sites says less than the method name does.
    fn offer_sweep_at(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        atom_id: u32,
        include_muted: bool,
    ) -> Result<OfferSweep, String> {
        let scene = self.mechanosynth_edit_scene(scope_path, node_id)?;
        if scene.structure.get_atom(atom_id).is_none() {
            return Err(format!("atom {atom_id} is not in the workpiece"));
        }
        // **A tool atom is not a host.** A tool is rewritten by the operations
        // that use it, at the pose its tagged atoms solve for; there is nothing
        // to place on it, and offering one would be offering a step the engine
        // refuses as `StepOnTool`.
        if let Participant::Tool(index) = scene.participant(atom_id) {
            let label = scene.label(Participant::Tool(index));
            return Err(format!(
                "that atom belongs to {label}; tools are rewritten by their operations, not \
                 placed on"
            ));
        }
        let library = self.mechanosynth_edit_library(scope_path, node_id)?;
        // The bindings make the sweep tool-aware: a `tip` row that fits the
        // clicked atom is additionally asked whether its tool is bound, in the
        // right state, and matches at its pose. `None` when nothing is wired to
        // `tools`, which leaves every row unannotated as before.
        let bindings = (!scene.bindings.is_empty()).then_some(scene.bindings.as_slice());
        // Cloned out of the node's data before the sweep, which needs the
        // library and the scene rather than the node — and before the `&mut`
        // borrow that stores the result. **The stored set either way**: an
        // including-muted sweep still has to say which of the rows it brought
        // back were the muted ones.
        let muted: BTreeSet<String> = self
            .mechanosynth_edit_data(scope_path, node_id)
            .ok_or("Not a mechanosynth_edit node")?
            .muted
            .clone();
        let admit = |operation: &Operation| include_muted || !muted.contains(&operation.name);
        let offers = applicable_ops_where(
            &scene.structure,
            &library,
            atom_id,
            resolve_tolerance(&library),
            bindings,
            &admit,
        );
        // Counted against the **wired library**, not against the stored set: a
        // muted name the library does not define was never going to be swept,
        // so reporting it would overstate what was hidden.
        let skipped_muted = library
            .ops
            .iter()
            .filter(|operation| !admit(operation))
            .count();
        let sweep = offer_sweep(
            &scene.structure,
            &library,
            atom_id,
            &offers,
            skipped_muted,
            &muted,
        );

        let data = self
            .mechanosynth_edit_data_mut(scope_path, node_id)
            .ok_or("Not a mechanosynth_edit node")?;
        data.placement.anchor = Some(atom_id);
        data.placement.offers = offers;
        Ok(sweep)
    }

    /// Which atom of *this* node's displayed workpiece a viewport ray hits, and
    /// where it is.
    ///
    /// The tool's entry points take an atom id, so this is the step that turns
    /// a click into one. It is deliberately scoped to the editor's own node: an
    /// atom of some other displayed structure is not a host the tool may place
    /// on, and the scene-wide hit test would hand back an id that means
    /// something else in this node's `result`.
    ///
    /// The **position and element travel with the id** because the popup is
    /// anchored to the atom and has to follow it as the camera moves — and the
    /// path that opens the popup with a candidate list has no offer sweep to
    /// take them from.
    pub fn mechanosynth_edit_anchor_at_ray(
        &self,
        scope_path: &[u64],
        node_id: u64,
        ray_origin: DVec3,
        ray_direction: DVec3,
    ) -> Option<AnchorInfo> {
        self.mechanosynth_edit_data(scope_path, node_id)?;
        let (atom_id, structure) = self.hit_test_node_atomic_structure(
            &crate::node_network::NodeRef::scoped(scope_path, node_id),
            &ray_origin,
            &ray_direction,
        )?;
        let atom = structure.get_atom(atom_id)?;
        Some(AnchorInfo {
            atom_id,
            position: atom.position,
            atomic_number: atom.atomic_number,
        })
    }

    /// Takes one candidate — `index` of `op`'s list — from whatever the last
    /// click produced, and commits it.
    ///
    /// The operation is named as well as the index because an offer list spans
    /// several operations, and **an operation the last offer list reported as a
    /// near miss is refused here**: outside the gate the library is stating it
    /// has not computed this situation, and there is deliberately no cast past
    /// that.
    pub fn mechanosynth_edit_choose(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        op: &str,
        index: usize,
    ) -> Result<usize, String> {
        let candidate = {
            let data = self
                .mechanosynth_edit_data(scope_path, node_id)
                .ok_or("Not a mechanosynth_edit node")?;
            let row = data
                .placement
                .offers
                .iter()
                .find(|row| row.op == op)
                .ok_or_else(|| format!("'{op}' is not in the current offer list"))?;
            if !row.fits {
                return Err(format!(
                    "'{op}' is {:.2} Å off here; this host is not an environment it was \
                     calculated for. Add the variant, or loosen the library's tolerance.",
                    row.best_residual
                ));
            }
            // A tool-blocked row is refused for the same reason a near miss is:
            // it cannot be committed, because its tool side would not match.
            if let Some(tool) = row.tool.as_ref().filter(|tool| !tool.ready) {
                return Err(format!(
                    "'{op}' cannot be performed here: {}",
                    tool.reason
                        .clone()
                        .unwrap_or_else(|| format!("{} is not ready", tool.tool_type))
                ));
            }
            // **Refused at the row before the index is looked at**, when every
            // way of placing it is refused: which of several impossible
            // placements was asked for is not the answer the user needs.
            if let Some(reason) = row.blocked() {
                return Err(format!("'{op}' cannot be placed here: {reason}"));
            }
            let candidate = row
                .candidates
                .get(index)
                .ok_or_else(|| {
                    format!(
                        "'{op}' has {} candidates here, so index {index} is out of range",
                        row.candidates.len()
                    )
                })?
                .clone();
            // **And by candidate**, for the mixed row the steric check is for:
            // one orientation of a donation is clean and the other lands in the
            // bulk, so the row is offerable and that one placement is not.
            if let Some(refusal) = candidate.refusal {
                return Err(format!(
                    "that way of placing '{op}' is refused: {}",
                    refusal.reason(op)
                ));
            }
            candidate
        };
        self.commit_candidate(scope_path, node_id, &candidate)
    }

    /// Selects one row of the open list for **preview**: its ghost atoms go
    /// into the transient state, and the next `eval(decorate)` hands them to
    /// the decorator.
    ///
    /// Taken on a *click*, never on hover. The preview is a real object in the
    /// scene rather than a projected overlay, so showing it costs one
    /// evaluation and one re-tessellation of the workpiece — cheap once per
    /// deliberate choice, ruinous once per mouse-move. That trade is the whole
    /// reason the row list activates on click.
    ///
    /// A **near miss** previews like anything else and is flagged, because
    /// seeing why a variant does not fit here is exactly what makes a library
    /// of environment variants learnable. Placing it stays refused.
    pub fn mechanosynth_edit_select_preview(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        op: &str,
        index: usize,
    ) -> Result<(), String> {
        let workpiece = self.mechanosynth_edit_scene(scope_path, node_id)?.structure;
        let library = self.mechanosynth_edit_library(scope_path, node_id)?;
        let operation = library
            .get(op)
            .ok_or_else(|| format!("{}: unknown operation '{op}'", library.file))?;

        let (candidate, near_miss) = {
            let data = self
                .mechanosynth_edit_data(scope_path, node_id)
                .ok_or("Not a mechanosynth_edit node")?;
            {
                let row = data
                    .placement
                    .offers
                    .iter()
                    .find(|row| row.op == op)
                    .ok_or_else(|| format!("'{op}' is not in the current offer list"))?;
                // A near miss keeps its best rejected fit in `near_miss`
                // rather than mixed in with placeable candidates, so the
                // preview has to look there for it.
                match row.preview_at(index) {
                    // Amber for a **refused** candidate too, not only for a
                    // near miss: the colour means "this is being shown, not
                    // placed", which is as true of a placement that lands in
                    // the bulk as of one outside the gate.
                    Some(candidate) => {
                        (candidate.clone(), !row.fits || candidate.refusal.is_some())
                    }
                    None => return Err(format!("'{op}' has no candidate {index} here")),
                }
            }
        };

        let ghosts = preview_atoms(&workpiece, operation, &candidate);
        let bonds = preview_bonds(&workpiece, operation, &candidate);
        let data = self
            .mechanosynth_edit_data_mut(scope_path, node_id)
            .ok_or("Not a mechanosynth_edit node")?;
        data.placement.preview_ghosts = ghosts;
        data.placement.preview_bonds = bonds;
        data.placement.preview_near_miss = near_miss;
        Ok(())
    }

    /// Drops the preview without closing the list.
    pub fn mechanosynth_edit_clear_preview(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
    ) -> Result<(), String> {
        let data = self
            .mechanosynth_edit_data_mut(scope_path, node_id)
            .ok_or("Not a mechanosynth_edit node")?;
        data.placement.clear_preview();
        Ok(())
    }

    /// Back to Idle: nothing inserted, no list, no preview.
    pub fn mechanosynth_edit_cancel(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
    ) -> Result<(), String> {
        let data = self
            .mechanosynth_edit_data_mut(scope_path, node_id)
            .ok_or("Not a mechanosynth_edit node")?;
        data.placement.reset();
        Ok(())
    }

    /// Which state the tool is in, for the panel's prompt.
    pub fn mechanosynth_edit_tool_state(
        &self,
        scope_path: &[u64],
        node_id: u64,
    ) -> Option<ToolState> {
        Some(
            self.mechanosynth_edit_data(scope_path, node_id)?
                .placement
                .state(),
        )
    }

    /// Inserts a candidate's step after the cursor, inheriting the build
    /// metadata of the step before it, and returns the tool to Idle.
    ///
    /// It deliberately does **not** leave the operation armed for the next
    /// click. The library splits a reaction into one operation per host
    /// environment (`si_donate_dimer`, `si_donate_site`, …), so the *same*
    /// operation is usually the wrong one at the next site, and a mode that
    /// silently changes what the next click means costs more than the clicks it
    /// saves. Fast repetition is a real need and wants its own design — a
    /// family applied automatically, or a family armed as a tool with the atoms
    /// it fits highlighted — not this.
    fn commit_candidate(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        candidate: &Candidate,
    ) -> Result<usize, String> {
        let (index, previous) = {
            let data = self
                .mechanosynth_edit_data(scope_path, node_id)
                .ok_or("Not a mechanosynth_edit node")?;
            let index = data.applied();
            // The template is the step before the insertion point. When the
            // block is empty that is the last step of the wired prefix, so a
            // hand-authored continuation of a generated block inherits its
            // chapter rather than starting a blank one.
            let previous = index
                .checked_sub(1)
                .and_then(|before| data.authored.get(before))
                .map(|authored| authored.step.clone());
            (index, previous)
        };
        let previous = match previous {
            Some(step) => Some(step),
            None => self.mechanosynth_edit_prefix(scope_path, node_id).pop(),
        };

        let mut step = AuthoredStep::from_candidate(candidate);
        if let Some(previous) = &previous {
            step.inherit_metadata_from(previous);
        }
        self.mechanosynth_edit_insert_step(scope_path, node_id, index, step)?;

        let data = self
            .mechanosynth_edit_data_mut(scope_path, node_id)
            .ok_or("Not a mechanosynth_edit node")?;
        data.placement.reset();
        Ok(index)
    }
}
