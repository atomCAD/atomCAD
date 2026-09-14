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
    AuthoredStep, MechanosynthEditData, OPS_PIN, STEPS_PIN, ToolState,
};
use crate::structure_designer::StructureDesigner;
use crate::undo::commands::mechanosynth_edit_block::{
    MechanosynthEditBlockCommand, MetadataEditKey,
};
use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::mechanosynth::{
    Applicability, Candidate, GhostAtom, OpLibrary, Step, applicable_ops, place, preview_atoms,
    resolve_tolerance, steps_applied,
};
use glam::DVec3;

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
}

/// What a pick did. The single-candidate case commits on the spot, because
/// asking a user to confirm the only possibility is a click for nothing.
#[derive(Debug, Clone, PartialEq)]
pub enum PickOutcome {
    /// One candidate; the step is already in the block at `index`.
    Committed { index: usize },
    /// Several candidates, held in the node's transient state awaiting a
    /// choice.
    Candidates(Vec<CandidateRow>),
    /// The armed operation does not fit here. The offers are what *does*, so a
    /// click on the wrong atom self-corrects in one more click rather than
    /// becoming a message to interpret.
    NoFit { message: String, offers: OfferSweep },
}

/// Builds the popup's view of a sweep. `workpiece` and `library` are the ones
/// the sweep ran against, so the ghosts and the residuals describe the same
/// state.
fn offer_sweep(
    workpiece: &AtomicStructure,
    library: &OpLibrary,
    atom_id: u32,
    rows: &[Applicability],
) -> OfferSweep {
    let anchor = workpiece
        .get_atom(atom_id)
        .expect("the clicked atom exists");
    OfferSweep {
        anchor_atom_id: atom_id,
        anchor_position: anchor.position,
        anchor_atomic_number: anchor.atomic_number,
        rows: rows
            .iter()
            .map(|row| {
                let preview = row.preview();
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
                    ghost: library
                        .get(&row.op)
                        .map(|op| preview_atoms(workpiece, op, preview))
                        .unwrap_or_default(),
                }
            })
            .collect(),
    }
}

/// Builds the candidate list's view. Same contract as [`offer_sweep`].
fn candidate_rows(
    workpiece: &AtomicStructure,
    library: &OpLibrary,
    op: &str,
    candidates: &[Candidate],
) -> Vec<CandidateRow> {
    let operation = library.get(op);
    candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| CandidateRow {
            index,
            residual: candidate.residual,
            exact: candidate.exact,
            mirrored: candidate.mirrored,
            approximate: candidate.approximate,
            ghost: operation
                .map(|operation| preview_atoms(workpiece, operation, candidate))
                .unwrap_or_default(),
        })
        .collect()
}

/// Which metadata field a chip edit writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepMetadataField {
    Note,
    Method,
    Phase,
    Layer,
    Site,
}

impl StepMetadataField {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "note" => Some(Self::Note),
            "method" => Some(Self::Method),
            "phase" => Some(Self::Phase),
            "layer" => Some(Self::Layer),
            "site" => Some(Self::Site),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Method => "method",
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

    /// The workpiece the tool acts on: the node's own `result`, i.e. the state
    /// at the cursor, which is what the user is looking at.
    fn mechanosynth_edit_workpiece(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
    ) -> Result<AtomicStructure, String> {
        match self.evaluate_node_output(scope_path, node_id, 0) {
            crate::evaluator::network_result::NetworkResult::Error(message) => Err(message),
            other => other
                .extract_atomic()
                .ok_or_else(|| "the node produced no atoms".to_string()),
        }
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
                    StepMetadataField::Method => authored_step.step.method = text,
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
        data.placement.clear_query();
        self.set_dirty(true);
        // A scrub must not merge into the next chip edit.
        self.pending_step_metadata_edit = None;
        Ok(())
    }

    // ========================================================================
    // The placement tool
    // ========================================================================

    /// The atom-first entry point: what the wired library can do at `atom_id`.
    /// Never an error for "nothing applies" — an empty list is an answer.
    pub fn mechanosynth_edit_offers(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        atom_id: u32,
    ) -> Result<OfferSweep, String> {
        let workpiece = self.mechanosynth_edit_workpiece(scope_path, node_id)?;
        if workpiece.get_atom(atom_id).is_none() {
            return Err(format!("atom {atom_id} is not in the workpiece"));
        }
        let library = self.mechanosynth_edit_library(scope_path, node_id)?;
        let offers = applicable_ops(&workpiece, &library, atom_id, resolve_tolerance(&library));
        let sweep = offer_sweep(&workpiece, &library, atom_id, &offers);

        let data = self
            .mechanosynth_edit_data_mut(scope_path, node_id)
            .ok_or("Not a mechanosynth_edit node")?;
        data.placement.anchor = Some(atom_id);
        data.placement.offers = offers;
        data.placement.candidates.clear();
        data.placement.candidates_op = None;
        Ok(sweep)
    }

    /// Arms the tool with one operation, for the repeat flow.
    pub fn mechanosynth_edit_arm(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        op: &str,
    ) -> Result<(), String> {
        let library = self.mechanosynth_edit_library(scope_path, node_id)?;
        if library.get(op).is_none() {
            return Err(format!("{}: unknown operation '{op}'", library.file));
        }
        let op = op.to_string();
        let data = self
            .mechanosynth_edit_data_mut(scope_path, node_id)
            .ok_or("Not a mechanosynth_edit node")?;
        data.placement.clear_query();
        data.placement.armed = Some(op);
        Ok(())
    }

    /// Places the armed operation at `atom_id`.
    pub fn mechanosynth_edit_pick(
        &mut self,
        scope_path: &[u64],
        node_id: u64,
        atom_id: u32,
    ) -> Result<PickOutcome, String> {
        let op = self
            .mechanosynth_edit_data(scope_path, node_id)
            .ok_or("Not a mechanosynth_edit node")?
            .placement
            .armed
            .clone()
            .ok_or("no operation is armed — click an atom to see what applies")?;

        let workpiece = self.mechanosynth_edit_workpiece(scope_path, node_id)?;
        if workpiece.get_atom(atom_id).is_none() {
            return Err(format!("atom {atom_id} is not in the workpiece"));
        }
        let library = self.mechanosynth_edit_library(scope_path, node_id)?;
        let tolerance = resolve_tolerance(&library);

        match place(&workpiece, &library, &op, atom_id, tolerance) {
            Ok(candidates) => {
                if candidates.len() == 1 {
                    let index = self.commit_candidate(scope_path, node_id, &candidates[0])?;
                    return Ok(PickOutcome::Committed { index });
                }
                let rows = candidate_rows(&workpiece, &library, &op, &candidates);
                let data = self
                    .mechanosynth_edit_data_mut(scope_path, node_id)
                    .ok_or("Not a mechanosynth_edit node")?;
                data.placement.anchor = Some(atom_id);
                data.placement.offers.clear();
                data.placement.candidates = candidates;
                data.placement.candidates_op = Some(op);
                Ok(PickOutcome::Candidates(rows))
            }
            // A failed pick is not the end of the interaction: the same click
            // becomes the atom-first query, so the popup that opens is headed
            // by the failure and lists what does fit beneath it.
            Err(failure) => {
                let message = failure.to_string();
                let offers = applicable_ops(&workpiece, &library, atom_id, tolerance);
                let sweep = offer_sweep(&workpiece, &library, atom_id, &offers);
                let data = self
                    .mechanosynth_edit_data_mut(scope_path, node_id)
                    .ok_or("Not a mechanosynth_edit node")?;
                data.placement.anchor = Some(atom_id);
                data.placement.offers = offers;
                data.placement.candidates.clear();
                data.placement.candidates_op = None;
                Ok(PickOutcome::NoFit {
                    message,
                    offers: sweep,
                })
            }
        }
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
            let placement = &data.placement;
            let from_candidates = (placement.candidates_op.as_deref() == Some(op))
                .then(|| placement.candidates.get(index))
                .flatten();
            match from_candidates {
                Some(candidate) => candidate.clone(),
                None => {
                    let row = placement
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
                    row.candidates
                        .get(index)
                        .ok_or_else(|| {
                            format!(
                                "'{op}' has {} candidates here, so index {index} is out of range",
                                row.candidates.len()
                            )
                        })?
                        .clone()
                }
            }
        };
        self.commit_candidate(scope_path, node_id, &candidate)
    }

    /// Back to Idle: nothing inserted, nothing armed.
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
    /// metadata of the step before it, and leaves the tool armed with the same
    /// operation so a run of identical placements is one click each.
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
        let op = step.step.op.clone();
        self.mechanosynth_edit_insert_step(scope_path, node_id, index, step)?;

        let data = self
            .mechanosynth_edit_data_mut(scope_path, node_id)
            .ok_or("Not a mechanosynth_edit node")?;
        data.placement.clear_query();
        data.placement.armed = Some(op);
        Ok(index)
    }
}
