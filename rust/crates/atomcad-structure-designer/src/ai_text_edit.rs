//! The AI text-edit choke point: parse an AI-submitted script, apply it to the
//! active network, validate, lay out, log and make it undoable.
//!
//! Every AI edit arrives here regardless of transport — the HTTP server's
//! `/edit` handler, the CLI REPL's `edit` / `replace` modes, and any future
//! direct FFI caller all funnel through `ai_edit_network` in
//! `rust/src/api/structure_designer/ai_assistant_api.rs`, which is now a thin
//! wrapper over [`StructureDesigner::ai_text_edit`] plus a refresh.
//!
//! **Why the orchestration lives down here rather than in `api/`.** It is
//! ordinary domain work — edit, validate, lay out, record — with exactly one
//! api-level step left behind (the scene refresh), and down here it is
//! reachable from a plain `StructureDesigner::new()`. In `api/` it could only
//! be exercised through the `CAD_INSTANCE` global, which is to say not at all:
//! `doc/design_ai_edit_history.md`'s Phase 1 test list — "a merge edit records
//! both snapshots and the submitted code verbatim", "a write-locked edit
//! records a failed entry" — is only writable against a callable function.
//! `rust/AGENTS.md` says the same thing from the other direction: API wrappers
//! are thin, and the underlying core function is what gets tested.

use crate::ai_edit_log::{
    AI_EDIT_COMMAND_DESCRIPTION, AiEditRecord, DeltaCounts, LayoutOutcome, LayoutPath,
};
use crate::layout;
use crate::layout::LayoutReport;
use crate::network_validator::validate_network;
use crate::node_network::NodeNetwork;
use crate::node_type_registry::NodeTypeRegistry;
use crate::scoped_validation_errors::{collect_scoped_validation_errors, error_node_path};
use crate::serialization::node_networks_serialization::node_network_to_serializable;
use crate::structure_designer::StructureDesigner;
use crate::text_format::{
    EditResult, NamePath, PositionSnapshot, edit_network_with_layout as text_edit_network,
    serialize_network, snapshot_node_positions, unique_node_names,
};
use crate::undo::UndoRefreshMode;
use crate::undo::commands::text_edit_network::TextEditNetworkCommand;

/// What [`StructureDesigner::ai_text_edit`] hands back to its caller.
pub struct AiTextEditOutcome {
    /// The result to report to the AI, verbatim.
    pub result: EditResult,
    /// The layout pass's report, the result's `layout` section
    /// (`doc/design_deliberate_node_placement.md` A3). `None` when there is
    /// nothing to say: no existing node moved and no finding or warning arose
    /// (and on every edit that did not apply), so an edit that leaves the
    /// drawing alone says nothing about layout. Kept beside `result`, never in
    /// it: `EditResult` is the editor's report, this is the layout pass's.
    pub layout: Option<LayoutReport>,
    /// The edit was a preview (`--dry-run`): everything above is what it would
    /// have done, and nothing of it stayed.
    pub dry_run: bool,
    /// The edit reached the network, so the caller owes it a scene refresh.
    /// False on the three rejection paths, which change nothing and so must not
    /// trigger one, and on a dry run, which reverted itself.
    pub needs_refresh: bool,
}

impl AiTextEditOutcome {
    /// The JSON the AI reads: the [`EditResult`] fields, with the layout
    /// report under `layout` when there is one and `"dry_run": true` on a
    /// preview. One object, so a dry run's JSON is the real edit's plus one
    /// key.
    pub fn to_json(&self) -> String {
        let mut value = match serde_json::to_value(&self.result) {
            Ok(value) => value,
            Err(e) => {
                return format!(
                    r#"{{"success":false,"errors":["Failed to serialize result: {}"]}}"#,
                    e
                );
            }
        };
        if let Some(object) = value.as_object_mut() {
            if let Some(layout) = &self.layout {
                object.insert("layout".to_string(), layout.to_json());
            }
            if self.dry_run {
                object.insert("dry_run".to_string(), serde_json::Value::Bool(true));
            }
        }
        value.to_string()
    }

    /// An edit that never reached the network.
    fn rejected(errors: Vec<String>, dry_run: bool) -> Self {
        Self {
            result: EditResult {
                success: false,
                applied: false,
                network_valid: None,
                nodes_created: vec![],
                nodes_updated: vec![],
                nodes_deleted: vec![],
                connections_made: vec![],
                description_set: None,
                summary_set: None,
                output_set: None,
                errors,
                warnings: vec![],
                preexisting_errors: vec![],
                preexisting_warnings: vec![],
                errors_fixed: vec![],
            },
            layout: None,
            dry_run,
            needs_refresh: false,
        }
    }
}

impl StructureDesigner {
    /// Apply an AI-submitted text-format script to the active node network.
    ///
    /// `replace` selects whole-network replacement over an incremental merge.
    ///
    /// The whole call is **one undo step** (`AI edit network`), so a bad edit —
    /// including one that emptied a zone body — is recoverable with Ctrl+Z
    /// (`doc/design_hof_body_text_format.md` Phase 5), and **one entry in the
    /// session's AI edit log** (`doc/design_ai_edit_history.md` Phase 1),
    /// including on the three paths that reject it outright.
    pub fn ai_text_edit(&mut self, code: &str, replace: bool) -> AiTextEditOutcome {
        self.ai_text_edit_with_options(code, replace, false)
    }

    /// [`ai_text_edit`](Self::ai_text_edit), optionally as a **dry run**
    /// (`doc/design_deliberate_node_placement.md`, "--dry-run").
    ///
    /// A dry run runs the entire edit (parse, apply, validate, layout pass)
    /// and returns the same outcome with `dry_run` set, then reverts it with
    /// the edit's own undo restore (`TextEditNetworkCommand::restore_before`,
    /// through [`revert_in_place`](Self::revert_in_place)). Nothing stays: no
    /// undo entry, no dirty flag, no AI-log record (not even for a rejection),
    /// no scene refresh.
    pub fn ai_text_edit_with_options(
        &mut self,
        code: &str,
        replace: bool,
        dry_run: bool,
    ) -> AiTextEditOutcome {
        // --- Rejection path 1: no active network -------------------------
        //
        // All three rejection paths are logged, not dropped (D1). A rejected
        // edit is exactly the kind of AI-facing feedback the log exists to
        // surface, and the one the AI is most likely to have handled badly.
        let network_name = match &self.active_node_network_name {
            Some(name) => name.clone(),
            None => {
                return self.reject_ai_edit(
                    String::new(),
                    code,
                    replace,
                    dry_run,
                    vec!["No active node network".to_string()],
                );
            }
        };

        // --- Rejection path 2: the CLI write lock ------------------------
        if self.is_cli_write_locked(&network_name) {
            let message = format!(
                "Write access to '{}' is locked. To modify this network, ask the user to unlock it from the GUI.",
                network_name
            );
            return self.reject_ai_edit(network_name, code, replace, dry_run, vec![message]);
        }

        // --- Rejection path 2b: linked content is read-only --------------
        // (`doc/design_library_linking.md` §6). Logged like the CLI lock.
        if let Err(message) = self.ensure_editable(&network_name) {
            return self.reject_ai_edit(network_name, code, replace, dry_run, vec![message]);
        }

        // --- The log's "before" pair (D2, D9) ----------------------------
        //
        // Taken while the network is still in the registry and before the
        // first statement runs. `serialize_network` is the *AI text format* —
        // the same text `query` would have returned at this moment — not
        // `.cnnd`; that is what makes the pair comparable across a `--replace`,
        // which mints a fresh id for every node and so defeats any id-based
        // delta. `snapshot_node_positions` is the editor's own identity walk,
        // shared so the layout measurement and the identity match it measures
        // cannot disagree.
        let (before_text, before_positions, before_wires, before_errors) = match self
            .node_type_registry
            .node_networks
            .get(&network_name)
        {
            Some(network) => (
                serialize_network(network, &self.node_type_registry, Some(&network_name)),
                snapshot_node_positions(network, &self.node_type_registry),
                // The other half of the incremental pass's "before": a
                // wire belongs to neither of its ends, so it cannot live in
                // the per-node identity map. Keyed by name path like
                // everything else, so a `--replace` does not report the
                // whole network as newly wired.
                layout::collect_all_wires(network),
                // The validation baseline: what was already broken is the
                // user's, not this edit's (the same rule as the layout
                // findings). The stored errors of the last validation pass.
                located_validation_errors(network),
            ),
            // --- Rejection path 3: the network is gone ---------------
            None => {
                let message = format!("Network '{}' not found", network_name);
                return self.reject_ai_edit(network_name, code, replace, dry_run, vec![message]);
            }
        };

        // Temporarily remove the network from the registry to avoid borrow
        // conflicts. This is necessary because `text_edit_network` needs
        // `&mut NodeNetwork` (the network being edited) *and*
        // `&NodeTypeRegistry` (to look node types up), and the network lives
        // inside the registry's `node_networks` map.
        let Some(mut network) = self.node_type_registry.node_networks.remove(&network_name) else {
            // Unreachable: the snapshot above just read this network out of the
            // same map. Kept as a rejection rather than an unwrap so a future
            // reordering degrades to a logged failure instead of a panic.
            let message = format!("Network '{}' not found", network_name);
            return self.reject_ai_edit(network_name, code, replace, dry_run, vec![message]);
        };

        // Snapshot the whole network BEFORE the edit, while it is still out of
        // the registry (`node_network_to_serializable` wants `&mut` the network
        // and `&` the registry's built-in types, which is exactly the borrow
        // split the remove above already bought). The snapshot carries every
        // node's `zone`, so one command covers body edits too
        // (`doc/design_hof_body_text_format.md` Phase 5).
        //
        // Not to be confused with `before_text`: this pair is the *undo
        // command's*, in the `.cnnd` representation, and exists to be restored;
        // the text pair exists to be read.
        let before_snapshot = node_network_to_serializable(
            &mut network,
            &self.node_type_registry.built_in_node_types,
            None,
            false,
        )
        .ok();
        // An edit can delete or reorder `parameter`s, which drops the
        // instances' wires into them in other networks (the call-site repair);
        // the undo step puts them back from this.
        let call_sites =
            crate::network_validator::capture_call_sites(&self.node_type_registry, &network_name);

        // A dry run reverts itself from the snapshot just taken, so without one
        // it cannot promise to leave nothing behind. Refuse before anything
        // is applied.
        if dry_run && before_snapshot.is_none() {
            self.node_type_registry
                .node_networks
                .insert(network_name.clone(), network);
            let message = format!(
                "Cannot preview an edit of '{}': the network could not be snapshotted",
                network_name
            );
            return self.reject_ai_edit(network_name, code, replace, dry_run, vec![message]);
        }

        // An edit is all or nothing: the editor carries on past a failed
        // statement, so on an error the untouched copy goes back instead. A
        // clone, not the snapshot above, because it keeps runtime-only node
        // state (`ai_placed`) that the `.cnnd` form drops; bodies are behind
        // an `Arc`, so it is cheap.
        let original = network.clone();

        // Apply the edit commands. The layout statements come back beside the
        // result: the editor does not act on them, the layout pass below does
        // (`doc/design_deliberate_node_placement.md` A2).
        let (mut result, layout_statements) =
            text_edit_network(&mut network, &self.node_type_registry, code, replace);

        // Whether the statements parsed and applied. The gates below are about
        // *what the editor did*, so they keep asking this — `result.success` is
        // about to mean something stricter (D15), and an edit that landed still
        // needs its layout and its dirty flag.
        let edit_applied = result.success;

        if !edit_applied {
            self.node_type_registry
                .node_networks
                .insert(network_name.clone(), original);
            return self.reject_applied_edit(
                network_name,
                code,
                replace,
                dry_run,
                result,
                before_text,
                &before_positions,
            );
        }

        // Put the network back into the registry
        self.node_type_registry
            .node_networks
            .insert(network_name.clone(), network);

        // Validate the edited network to update output_type and repair
        // arguments. This is necessary because `text_edit_network` doesn't
        // update `network.node_type.output_type` when the return node is set
        // via an `output <node>` statement. Without this, custom nodes using
        // this network won't render correctly.
        {
            let registry_ptr = &mut self.node_type_registry as *mut NodeTypeRegistry;
            unsafe {
                if let Some(network) = (*registry_ptr).node_networks.get_mut(&network_name) {
                    validate_network(network, &mut *registry_ptr, None);
                }
            }
        }

        // `success` means *parsed, applied, and broke nothing* — not merely
        // *parsed* (`doc/design_hof_body_text_format.md` D15). The verdict
        // above used to be computed for its side effects and thrown away, which
        // is how an edit that silently emptied a zone body could still report
        // `success: true`. Errors are reported with the offending node's full
        // path (D10) so the AI can find it, including inside a body.
        //
        // Only errors the edit *introduced* fail it. One the network already
        // had (matched by `path: message` against the baseline) goes to
        // `preexisting_*`: on a network that is broken elsewhere — a missing
        // linked library is the everyday case — every edit would otherwise
        // report `success: false`, and an AI soon learns to ignore the flag,
        // including on the edit that really did break something. The
        // whole-network verdict is `network_valid`.
        if let Some(network) = self.node_type_registry.node_networks.get(&network_name) {
            let after_errors = located_validation_errors(network);
            let split = split_against_baseline(&before_errors, after_errors);
            // The severity split is the existing one: blocking errors fail
            // the edit, non-blocking ones are advisory
            // (`project_nonblocking_validation_errors`).
            for error in split.new_errors {
                result.add_error(error);
            }
            for warning in split.new_warnings {
                result.add_warning(warning);
            }
            result.network_valid = Some(split.network_valid);
            result.preexisting_errors = split.preexisting_errors;
            result.preexisting_warnings = split.preexisting_warnings;
            result.errors_fixed = split.errors_fixed;
        }

        // --- Layout: the incremental pass (D1, D8) -----------------------
        //
        // Not a reflow. The pass takes the edit's own delta per scope, lays out
        // only the nodes it added, fits them into the existing drawing and
        // repairs locally; every other node keeps the position the user gave
        // it. That is why it has no preference gating it — it is repair, not
        // layout, and the thing the old `auto_layout_after_edit` switch existed
        // to avoid (a human's arrangement destroyed on every edit) is what this
        // pass does not do. A full reflow is still available, and is now only
        // ever user-invoked, through `layout_active_network`.
        //
        // It runs on **every scope**, inside-out, so a body edit is laid out
        // like a top-level one and the owning HOF's new footprint is repaired
        // in its parent afterwards (D12).
        let mut incremental = layout::IncrementalOutcome::default();
        let ran_layout = edit_applied;
        if ran_layout {
            let algorithm = self.preferences.layout_preferences.layout_algorithm.into();
            let registry_ptr = &self.node_type_registry as *const NodeTypeRegistry;
            unsafe {
                if let Some(network) = self.node_type_registry.node_networks.get_mut(&network_name)
                {
                    incremental = layout::layout_incremental_with_statements(
                        network,
                        &*registry_ptr,
                        &before_positions,
                        &before_wires,
                        algorithm,
                        &layout_statements,
                    );
                }
            }
        }

        // --- Ownership (`doc/design_deliberate_node_placement.md` D12) ----
        //
        // Every node this edit created is the AI's until a human positions
        // it. "Created" is the identity match the layout pass used: a name
        // path absent from the pre-edit snapshot, so a `--replace` that
        // recreates a node under its old name does not change its owner (the
        // editor carried the flag across with the rest of its state). Set
        // before the undo snapshot below, which does not record it anyway: the
        // flag is runtime-only, and a redo makes the nodes the user's.
        if edit_applied
            && let Some(network) = self.node_type_registry.node_networks.get_mut(&network_name)
        {
            mark_created_nodes_ai_placed(network, &before_positions);
        }

        // --- The log's "after" pair, and the layout outcome (D9) ---------
        //
        // Taken past validation and past layout, so an edit that applied but
        // left the network invalid shows its real effect.
        //
        // `LayoutPath::Incremental` now covers **every** scope: the pass walks
        // bodies as first-class scopes, so unlike the old `FullReflow` reading
        // it is not a claim about the root scope alone. A body node that moved
        // was moved by this pass, not left where it was created.
        let layout_path = if ran_layout {
            LayoutPath::Incremental
        } else {
            LayoutPath::None
        };
        let (after_text, mut layout_outcome) =
            match self.node_type_registry.node_networks.get(&network_name) {
                Some(network) => {
                    let after_positions =
                        snapshot_node_positions(network, &self.node_type_registry);
                    let mut outcome = LayoutOutcome::compute(
                        layout_path,
                        network,
                        &before_positions,
                        &after_positions,
                    );
                    // The counters the log has carried a slot for since its own
                    // Phase 1: what the edit *asked* layout to do, against the
                    // `moved` list of what layout then did.
                    if ran_layout {
                        let totals = incremental.totals;
                        outcome.delta = Some(DeltaCounts {
                            nodes_added: totals.nodes_added,
                            nodes_modified: totals.nodes_modified,
                            nodes_removed: totals.nodes_removed,
                            wires_added: totals.wires_added,
                            wires_removed: totals.wires_removed,
                        });
                    }
                    (
                        serialize_network(network, &self.node_type_registry, Some(&network_name)),
                        outcome,
                    )
                }
                None => (String::new(), LayoutOutcome::default()),
            };
        layout_outcome.attach_causes(&incremental.report);
        // The drawing's problems before and after, in the scopes where an
        // existing node moved: `findings` says only what this edit created, so
        // without these a relayout that fixes five crossings and keeps one
        // old one reads the same as one that fixes nothing. `original` is the
        // pre-edit clone the rollback would have used.
        if ran_layout
            && let Some(network) = self.node_type_registry.node_networks.get(&network_name)
        {
            incremental.report.totals = layout::finding_totals(
                &original,
                network,
                &self.node_type_registry,
                &incremental.report.moved,
            );
        }
        // A script of layout statements alone changes no text, but it moves
        // nodes: that is a change, which needs its undo step and dirty flag.
        let layout_moved =
            !incremental.report.moved.is_empty() || !incremental.report.placed.is_empty();
        // The report travels to the AI only when it has something to say.
        let layout_report =
            (ran_layout && incremental.report.is_worth_reporting()).then_some(incremental.report);

        // The editor's change lists are not the whole story: a `--replace`
        // clears the network *before* a single statement runs, so a script
        // that then creates nothing — empty, or comment-only, which is what a
        // `query` printout cut short at its first blank line amounts to — has
        // wiped the drawing while reporting every list empty. The text pair
        // the log carries is the total answer: if the network reads
        // differently now, something changed. (Text, not positions: layout
        // only moves a node when the text changed too.)
        let text_changed = before_text != after_text;

        // Push the log entry. Everything from the `EditResult` goes in verbatim
        // (D8) — including `description_set` / `summary_set` / `output_set`,
        // which are easy to drop and are the only trace an `output` re-point
        // leaves. The two verdicts are kept apart: `applied` is the editor's
        // own, `success` is that folded with the validation errors the edit
        // introduced, and `applied && !success` is "my edit landed and broke
        // something". Errors the network already had are not the edit's and
        // are not in `errors`.
        let mut record = AiEditRecord::applied_edit(
            network_name.clone(),
            code.to_string(),
            replace,
            edit_applied,
            before_text,
            after_text,
            layout_outcome,
        );
        record.success = result.success;
        record.nodes_created = result.nodes_created.clone();
        record.nodes_updated = result.nodes_updated.clone();
        record.nodes_deleted = result.nodes_deleted.clone();
        record.connections_made = result.connections_made.clone();
        record.description_set = result.description_set.clone();
        record.summary_set = result.summary_set.clone();
        record.output_set = result.output_set.clone();
        record.errors = result.errors.clone();
        record.warnings = result.warnings.clone();

        // A dry run ends here. Everything the AI is shown has been computed;
        // revert with the very restore the edit's undo command would run, and
        // record nothing: no log entry, no undo step, no dirty flag, no
        // refresh.
        if dry_run {
            if let Some(before) = before_snapshot {
                self.revert_in_place(UndoRefreshMode::Full, |ctx| {
                    TextEditNetworkCommand::restore_before(
                        ctx,
                        &network_name,
                        &before,
                        &call_sites,
                    );
                });
                // The restore rebuilt the network from its serialized form,
                // which does not carry the runtime-only `ai_placed` (D12). A
                // preview must not hand the AI's nodes to the user, so the
                // flags go back from the identity snapshot.
                if let Some(network) = self.node_type_registry.node_networks.get_mut(&network_name)
                {
                    restore_ai_placed(network, &before_positions);
                }
            }
            return AiTextEditOutcome {
                result,
                layout: layout_report,
                dry_run: true,
                needs_refresh: false,
            };
        }
        self.record_ai_edit(record);

        // Did the edit actually change anything? Gates both the dirty flag and
        // the undo command below.
        //
        // Deliberately keyed on `edit_applied`, not `result.success`: an edit
        // that applied and then failed *validation* is still on the network,
        // and a bad edit is precisely the one the user reaches for Ctrl+Z over.
        // Gating on the post-fold `success` would leave the most damaging edits
        // unrecoverable — the opposite of what that phase was for.
        let made_changes = edit_applied
            && (text_changed
                || layout_moved
                || !result.nodes_created.is_empty()
                || !result.nodes_updated.is_empty()
                || !result.nodes_deleted.is_empty()
                || !result.connections_made.is_empty()
                || result.description_set.is_some()
                || result.summary_set.is_some()
                || result.output_set.is_some());

        // Record the edit as one undo step. The after-snapshot is taken
        // **here**, past validation and past auto-layout, because `redo`
        // restores it verbatim and re-runs neither: captured any earlier, a
        // redo would put every node back at its pre-layout position.
        if made_changes {
            let after_snapshot = if let Some(mut network) =
                self.node_type_registry.node_networks.remove(&network_name)
            {
                let snapshot = node_network_to_serializable(
                    &mut network,
                    &self.node_type_registry.built_in_node_types,
                    None,
                    false,
                )
                .ok();
                self.node_type_registry
                    .node_networks
                    .insert(network_name.clone(), network);
                snapshot
            } else {
                None
            };
            if let (Some(before), Some(after)) = (before_snapshot, after_snapshot) {
                self.push_command(TextEditNetworkCommand {
                    network_name: network_name.clone(),
                    before_snapshot: before,
                    after_snapshot: after,
                    // Distinct from the in-app *Text* tab's label: the undo
                    // tooltip is the only place the user learns that the step
                    // they are about to revert came from the AI rather than
                    // from their own typing. Shared with the AI history log,
                    // which recognises this exact string to word its
                    // "edit #N undone" marker (D7).
                    description: AI_EDIT_COMMAND_DESCRIPTION,
                    call_sites,
                });
            }
        }

        // Mark that a full refresh is needed, since the network was edited
        // directly (bypassing StructureDesigner change tracking).
        self.mark_full_refresh();

        // Set the dirty flag if any modifications were made
        // (`text_edit_network` bypasses the normal edit methods that set it).
        if made_changes {
            self.set_dirty(true);
        }

        AiTextEditOutcome {
            result,
            layout: layout_report,
            dry_run: false,
            needs_refresh: true,
        }
    }

    /// The layout read-out of the active network
    /// (`doc/design_deliberate_node_placement.md` A1, `query --layout`): the
    /// boxes of the nodes `nodes` / `bbox` select in the scope `scope`, plus
    /// the drawing's extent and its inferred groups. The three arguments are
    /// as the transports deliver them (see [`layout::parse_readout_args`]),
    /// empty when absent.
    ///
    /// Read-only, like `query`: it reaches nothing but the network. A problem
    /// comes back as a `# Error: …` line, the way `query` reports one.
    pub fn query_active_layout(&self, scope: &str, nodes: &str, bbox: &str) -> String {
        self.query_active_layout_with(scope, nodes, bbox, false)
    }

    /// [`query_active_layout`](Self::query_active_layout), with the
    /// `# findings` section (`query --layout --findings`) appended when
    /// `findings` is set: every layout problem the scope has now, not only
    /// what an edit created ([`layout::findings_readout`]).
    pub fn query_active_layout_with(
        &self,
        scope: &str,
        nodes: &str,
        bbox: &str,
        findings: bool,
    ) -> String {
        let Some(name) = self.active_node_network_name.as_deref() else {
            return "# No active node network\n".to_string();
        };
        let Some(network) = self.node_type_registry.node_networks.get(name) else {
            return format!("# Network '{}' not found\n", name);
        };
        let registry = &self.node_type_registry;
        layout::parse_readout_args(scope, nodes, bbox)
            .and_then(|(scope, filter)| {
                let mut out = layout::layout_readout(network, registry, name, &scope, &filter)?;
                if findings {
                    out.push_str(&layout::findings_readout(
                        network, registry, name, &scope, &filter,
                    )?);
                }
                Ok(out)
            })
            .unwrap_or_else(|message| format!("# Error: {}\n", message))
    }

    /// The frame `query --layout --image` draws, for the same arguments as
    /// [`query_active_layout`](Self::query_active_layout). `Err` is a one-line
    /// message.
    pub fn active_layout_image_frame(
        &self,
        scope: &str,
        nodes: &str,
        bbox: &str,
    ) -> Result<layout::ImageFrame, String> {
        let name = self
            .active_node_network_name
            .as_deref()
            .ok_or_else(|| "no active node network".to_string())?;
        let network = self
            .node_type_registry
            .node_networks
            .get(name)
            .ok_or_else(|| format!("network '{}' not found", name))?;
        let (scope, filter) = layout::parse_readout_args(scope, nodes, bbox)?;
        layout::layout_image_frame(network, &self.node_type_registry, name, &scope, &filter)
    }

    /// Log an edit that never reached the network and build its `EditResult`.
    /// A dry run is not logged, rejected or not.
    fn reject_ai_edit(
        &mut self,
        network_name: String,
        code: &str,
        replace: bool,
        dry_run: bool,
        errors: Vec<String>,
    ) -> AiTextEditOutcome {
        if !dry_run {
            self.record_ai_edit(AiEditRecord::rejected(
                network_name,
                code.to_string(),
                replace,
                errors.clone(),
            ));
        }
        AiTextEditOutcome::rejected(errors, dry_run)
    }

    /// Log and report an edit the editor ran but found an error in, after the
    /// network has been put back as it was. Nothing of it stayed, so the
    /// result lists nothing created or changed; its errors and warnings are
    /// kept, with a closing line saying so. The log entry carries the network's
    /// text as both snapshots, so it says "unchanged" rather than "no state".
    #[allow(clippy::too_many_arguments)]
    fn reject_applied_edit(
        &mut self,
        network_name: String,
        code: &str,
        replace: bool,
        dry_run: bool,
        result: EditResult,
        before_text: String,
        before_positions: &PositionSnapshot,
    ) -> AiTextEditOutcome {
        let errors = rolled_back_errors(result.errors);
        if !dry_run {
            let layout = match self.node_type_registry.node_networks.get(&network_name) {
                Some(network) => LayoutOutcome::compute(
                    LayoutPath::None,
                    network,
                    before_positions,
                    before_positions,
                ),
                None => LayoutOutcome::default(),
            };
            let mut record = AiEditRecord::applied_edit(
                network_name,
                code.to_string(),
                replace,
                false,
                before_text.clone(),
                before_text,
                layout,
            );
            record.errors = errors.clone();
            record.warnings = result.warnings.clone();
            self.record_ai_edit(record);
        }
        let mut outcome = AiTextEditOutcome::rejected(errors, dry_run);
        outcome.result.warnings = result.warnings;
        outcome
    }
}

/// The message closing the error list of an edit that was rolled back.
pub const EDIT_NOT_APPLIED_MESSAGE: &str =
    "Nothing was applied: an edit with an error leaves the network unchanged";

/// `errors` plus [`EDIT_NOT_APPLIED_MESSAGE`].
pub fn rolled_back_errors(mut errors: Vec<String>) -> Vec<String> {
    errors.push(EDIT_NOT_APPLIED_MESSAGE.to_string());
    errors
}

/// Every validation error of `network`, bodies included, as the AI is shown it
/// (`path: message`, the path the text format addresses the node by; the bare
/// message for a network-level error), paired with whether it blocks. This
/// spelling is also the key that matches an error across an edit: node ids do
/// not survive a `--replace`, names do.
pub fn located_validation_errors(network: &NodeNetwork) -> Vec<(String, bool)> {
    collect_scoped_validation_errors(network)
        .into_iter()
        .map(|error| {
            let located = match error_node_path(network, &error.scope_path, error.node_id) {
                Some(path) => format!("{}: {}", path, error.error_text),
                None => error.error_text,
            };
            (located, error.blocking)
        })
        .collect()
}

/// The validation errors after an edit, split against those before it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ValidationSplit {
    /// Blocking errors the edit introduced: they fail it.
    pub new_errors: Vec<String>,
    /// Non-blocking errors the edit introduced.
    pub new_warnings: Vec<String>,
    /// Blocking errors that were there before and still are.
    pub preexisting_errors: Vec<String>,
    /// Non-blocking errors that were there before and still are.
    pub preexisting_warnings: Vec<String>,
    /// Blocking errors that were there before and are gone.
    pub errors_fixed: Vec<String>,
    /// No blocking error remains, new or old.
    pub network_valid: bool,
}

/// Split `after` against `before` (both from [`located_validation_errors`]).
/// An error is matched by its text and severity, as a multiset: two identical
/// errors before and three after is one new one. Order follows `after` (and
/// `before` for the fixed ones). An error whose wording the edit changed reads
/// as one fixed and one new, which is what it is to the reader.
pub fn split_against_baseline(
    before: &[(String, bool)],
    after: Vec<(String, bool)>,
) -> ValidationSplit {
    use std::collections::HashMap;
    let mut split = ValidationSplit {
        network_valid: !after.iter().any(|(_, blocking)| *blocking),
        ..ValidationSplit::default()
    };
    let mut baseline: HashMap<&(String, bool), usize> = HashMap::new();
    for error in before {
        *baseline.entry(error).or_default() += 1;
    }
    let mut remaining: HashMap<(String, bool), usize> = HashMap::new();
    for (text, blocking) in after {
        let key = (text, blocking);
        let preexisting = match baseline.get_mut(&key) {
            Some(count) if *count > 0 => {
                *count -= 1;
                true
            }
            _ => false,
        };
        *remaining.entry(key.clone()).or_default() += 1;
        let (text, blocking) = key;
        match (blocking, preexisting) {
            (true, false) => split.new_errors.push(text),
            (false, false) => split.new_warnings.push(text),
            (true, true) => split.preexisting_errors.push(text),
            (false, true) => split.preexisting_warnings.push(text),
        }
    }
    for error in before.iter().filter(|(_, blocking)| *blocking) {
        match remaining.get_mut(error) {
            Some(count) if *count > 0 => *count -= 1,
            _ => split.errors_fixed.push(error.0.clone()),
        }
    }
    split
}

/// Set [`ai_placed`](crate::node_network::Node::ai_placed) on every node of
/// `network`, bodies included, whose name path `before` does not hold: the
/// nodes an edit created. Names are the text format's
/// ([`unique_node_names`]), the key [`snapshot_node_positions`] uses, so this
/// and the layout pass agree on what is new.
fn mark_created_nodes_ai_placed(network: &mut NodeNetwork, before: &PositionSnapshot) {
    fn walk(network: &mut NodeNetwork, before: &PositionSnapshot, prefix: &mut NamePath) {
        let names = unique_node_names(network);
        let mut ids: Vec<u64> = network.nodes.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let Some(name) = names.get(&id) else {
                continue;
            };
            prefix.push(name.clone());
            let created = !before.contains_key(prefix.as_slice());
            let node = network.nodes.get_mut(&id).expect("id from this map");
            if created {
                node.ai_placed = true;
            }
            // A new HOF's body is all new; an old one's may hold new nodes.
            // Only descend where something can change, so an untouched body
            // keeps sharing its storage.
            let descend = node.zone.as_deref().is_some_and(|body| {
                let mut any = false;
                walk_names(body, prefix, &mut |path| any |= !before.contains_key(path));
                any
            });
            if descend && let Some(body) = node.zone_mut() {
                walk(body, before, prefix);
            }
            prefix.pop();
        }
    }
    /// Visit the name path of every node of `network`, bodies included.
    fn walk_names(network: &NodeNetwork, prefix: &mut NamePath, f: &mut impl FnMut(&[String])) {
        let names = unique_node_names(network);
        for (id, node) in &network.nodes {
            let Some(name) = names.get(id) else {
                continue;
            };
            prefix.push(name.clone());
            f(prefix);
            if let Some(body) = node.zone.as_deref() {
                walk_names(body, prefix, f);
            }
            prefix.pop();
        }
    }
    walk(network, before, &mut Vec::new());
}

/// Set every node's [`ai_placed`](crate::node_network::Node::ai_placed) to
/// what `snapshot` recorded for its name path (false for a path it lacks).
/// Bodies are only forked where a flag actually changes.
fn restore_ai_placed(network: &mut NodeNetwork, snapshot: &PositionSnapshot) {
    fn walk(network: &mut NodeNetwork, snapshot: &PositionSnapshot, prefix: &mut NamePath) {
        let names = unique_node_names(network);
        let mut ids: Vec<u64> = network.nodes.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let Some(name) = names.get(&id) else {
                continue;
            };
            prefix.push(name.clone());
            let node = network.nodes.get_mut(&id).expect("id from this map");
            node.ai_placed = snapshot
                .get(prefix.as_slice())
                .is_some_and(|state| state.ai_placed);
            let any_flagged = node.zone.as_deref().is_some_and(|_| {
                snapshot.iter().any(|(path, state)| {
                    state.ai_placed && path.len() > prefix.len() && path.starts_with(prefix)
                })
            });
            if any_flagged && let Some(body) = node.zone_mut() {
                walk(body, snapshot, prefix);
            }
            prefix.pop();
        }
    }
    walk(network, snapshot, &mut Vec::new());
}
