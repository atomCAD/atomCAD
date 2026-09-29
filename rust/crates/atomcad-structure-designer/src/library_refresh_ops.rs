//! *Refresh*, *Refresh all dependencies*, *Change file…* (retarget) and the
//! automatic check (`doc/design_library_linking.md` D7, D9, D10, §7): the
//! `StructureDesigner` operations of library linking Phase 3.
//!
//! One procedure (§7.1) serves them all: capture the interfaces the host is
//! wired against, snapshot every host network that refers to the affected
//! mounts, re-mount, reconcile the host's wiring from the captured interfaces
//! to the new ones, validate, snapshot again, and push one
//! [`RefreshDependenciesCommand`]. Rust decides everything here; Flutter only
//! decides *when* to call [`StructureDesigner::check_dependencies`].

use crate::library_links::{self, ImportSpec, MountStatus};
use crate::library_refresh::{
    self, DataFileKey, RefreshReport, detect_changes, direct_of, frozen_nodes_of,
    local_networks_referring_to, mount_direct, rebuild_data_watches, referenced_linked_names,
    reload_data_file_nodes, snapshot_mount, stamp_file,
};
use crate::node_network::{NodeNetwork, walk_all_nodes};
use crate::structure_designer::StructureDesigner;
use crate::undo::commands::refresh_dependencies::{MountSwap, RefreshDependenciesCommand};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// One direct mount to refresh, optionally from another file (retarget).
#[derive(Clone, Debug)]
pub struct RefreshTarget {
    pub alias: String,
    /// The new relative path for a retarget; `None` = the mount's own file.
    pub new_rel_path: Option<String>,
}

impl StructureDesigner {
    /// The automatic check (D7): called by the UI when the window regains
    /// focus, by a poll while it has focus, and after Save As. Stats every
    /// watched file; whatever changed is refreshed in **one** undoable
    /// command (D10) — or, while there is something to redo, **held** so that
    /// the redo history survives (D9): the mount is marked `ChangedOnDisk` and
    /// the change is reported once in `held`. `None` when nothing was
    /// refreshed, held or changed status.
    pub fn check_dependencies(&mut self) -> Option<RefreshReport> {
        // An open interaction (a drag, a comment being typed) is one undo
        // step in the making; a refresh pushed in the middle of it would
        // split it, and its snapshots would capture a half-dragged state.
        // The check is skipped as a whole — nothing observed, so `last_seen`
        // does not advance and the next check after the interaction ends
        // finds the change still pending (P4, "Rust-side interactions").
        if self.open_interaction().is_some() {
            return None;
        }
        let registry = &mut self.node_type_registry;
        if registry.library_links.is_empty() && registry.library_links.data_files.is_empty() {
            return None;
        }
        let detection = detect_changes(registry);
        let mut report = RefreshReport {
            status_changes: detection.status_changes,
            ..Default::default()
        };
        if !(detection.mounts.is_empty() && detection.host_files.is_empty()) {
            if self.undo_stack.can_redo() {
                let registry = &mut self.node_type_registry;
                for alias in &detection.mounts {
                    if let Some(m) = registry.library_links.get_mut(alias)
                        && m.status != MountStatus::ChangedOnDisk
                    {
                        m.status = MountStatus::ChangedOnDisk;
                        report.held.push(alias.clone());
                    }
                }
                for path in &detection.host_files {
                    let key = DataFileKey {
                        owner: None,
                        path: path.clone(),
                    };
                    if let Some(w) = registry.library_links.data_files.get_mut(&key)
                        && !w.held
                    {
                        w.held = true;
                        report.held.push(path.to_string_lossy().to_string());
                    }
                }
            } else {
                let targets: Vec<RefreshTarget> = detection
                    .mounts
                    .iter()
                    .map(|alias| RefreshTarget {
                        alias: alias.clone(),
                        new_rel_path: None,
                    })
                    .collect();
                match self.refresh_dependencies(&targets, &detection.host_files) {
                    Ok(r) => report.merge(r),
                    Err(e) => report.errors.push(e),
                }
            }
        }
        (!report.is_empty()).then_some(report)
    }

    /// The interaction the undo coalescing currently treats as one open
    /// step, if any — what [`check_dependencies`](Self::check_dependencies)
    /// holds a refresh for, and what refuses an explicit one. These are the
    /// interactions Rust knows about; the ones only Flutter knows (a text
    /// field being typed into, a modal dialog) are skipped by Flutter
    /// (`lib/structure_designer/AGENTS.md`). `mechanosynth_edit` keystroke
    /// coalescing is not listed: it is keyed on the undo push count, so any
    /// command pushed in between simply ends the run.
    pub fn open_interaction(&self) -> Option<&'static str> {
        if self.pending_move.is_some() {
            Some("a node drag")
        } else if self.pending_atom_edit_drag.is_some() {
            Some("an atom drag")
        } else if self.pending_gadget_drag.is_some() {
            Some("a gadget drag")
        } else if self.pending_node_data_drag.is_some() {
            Some("a property drag")
        } else if self.pending_zone_resize.is_some() {
            Some("a body resize")
        } else if self.pending_comment_edit.is_some() {
            Some("a comment edit")
        } else {
            None
        }
    }

    /// *Refresh* on a mount folder: re-reads the direct mount containing
    /// `mount_path` (its file, its nested libraries and their data files),
    /// re-hashing unconditionally. A user action like any edit: applied even
    /// while redo history exists (which it truncates). Nothing is pushed when
    /// nothing differs from what is in memory.
    pub fn refresh_library(&mut self, mount_path: &str) -> Result<RefreshReport, String> {
        if self
            .node_type_registry
            .library_links
            .get(mount_path)
            .is_none()
        {
            return Err(format!("no linked library '{}'", mount_path));
        }
        let alias = direct_of(&self.node_type_registry, mount_path);
        if !library_refresh::mount_differs_from_disk(&self.node_type_registry, &alias) {
            return Ok(self.settle_unchanged(&[alias], &BTreeSet::new()));
        }
        self.refresh_dependencies(
            &[RefreshTarget {
                alias,
                new_rel_path: None,
            }],
            &BTreeSet::new(),
        )
    }

    /// *File > Refresh all dependencies*: every direct mount and host data
    /// file that differs from what is in memory, in one command.
    pub fn refresh_all_dependencies(&mut self) -> Result<RefreshReport, String> {
        let registry = &self.node_type_registry;
        let fs = registry.library_links.fs();
        let aliases: Vec<String> = registry
            .library_links
            .direct_mounts()
            .map(|m| m.mount_path.clone())
            .collect();
        let (changed, unchanged): (Vec<String>, Vec<String>) = aliases
            .into_iter()
            .partition(|a| library_refresh::mount_differs_from_disk(registry, a));
        let host_files: BTreeSet<PathBuf> = registry
            .library_links
            .data_files
            .iter()
            .filter(|(k, _)| k.owner.is_none())
            .filter(|(k, w)| {
                let now = stamp_file(fs.as_ref(), &k.path);
                now.is_some()
                    && now.as_ref().map(|s| &s.blake3) != w.loaded.as_ref().map(|s| &s.blake3)
            })
            .map(|(k, _)| k.path.clone())
            .collect();
        let mut report = self.settle_unchanged(&unchanged, &host_files);
        if changed.is_empty() && host_files.is_empty() {
            return Ok(report);
        }
        let targets: Vec<RefreshTarget> = changed
            .into_iter()
            .map(|alias| RefreshTarget {
                alias,
                new_rel_path: None,
            })
            .collect();
        report.merge(self.refresh_dependencies(&targets, &host_files)?);
        Ok(report)
    }

    /// *Change file…* (retarget, §7): points the direct mount `alias` at the
    /// `.cnnd` at `path` (relative to the design's folder, or absolute) and
    /// brings it in like a refresh — one undo step whose undo points the
    /// alias back at the old file. Nothing in the host is renamed (D2). On a
    /// file that cannot be mounted, nothing changes.
    pub fn retarget_library(&mut self, alias: &str, path: &str) -> Result<RefreshReport, String> {
        match self.node_type_registry.library_links.get(alias) {
            Some(m) if m.is_direct() => {}
            Some(_) => {
                return Err(format!(
                    "'{}' is linked by another library; only direct links can be retargeted",
                    alias
                ));
            }
            None => return Err(format!("no linked library '{}'", alias)),
        }
        let host = self
            .host_file()
            .ok_or_else(|| "save the design before linking a library".to_string())?;
        let rel_path = self.library_rel_path(&host, path)?;
        self.refresh_dependencies(
            &[RefreshTarget {
                alias: alias.to_string(),
                new_rel_path: Some(rel_path),
            }],
            &BTreeSet::new(),
        )
    }

    /// The report of the last file open (D7, D13) — what reconciling the
    /// host's wiring against its recorded interfaces did, which nodes are
    /// frozen, which libraries are not loaded or changed since the file was
    /// saved. Drained like `take_load_param_id_repairs`; `None` when there is
    /// nothing to say.
    pub fn take_load_library_report(&mut self) -> Option<RefreshReport> {
        let report = std::mem::take(&mut self.node_type_registry.library_links.load_report);
        (!report.is_empty()).then_some(report)
    }

    /// Completes the load report after `load_node_networks` validated
    /// everything, starts watching the data files, and marks the design dirty
    /// when the open reconciled anything (§7.1 step 7: the file in memory is
    /// not the file on disk).
    pub(crate) fn finish_library_load(&mut self) {
        let registry = &mut self.node_type_registry;
        rebuild_data_watches(registry);
        let mut report = std::mem::take(&mut registry.library_links.load_report);
        for (name, before) in std::mem::take(&mut registry.library_links.load_images) {
            if let Some(network) = registry.node_networks.get(&name) {
                library_refresh::report_wires_removed_by_repair(
                    &before,
                    network,
                    registry,
                    &name,
                    &mut report,
                );
            }
        }
        let locals: Vec<String> = registry
            .node_networks
            .keys()
            .filter(|n| registry.library_links.mount_containing(n).is_none())
            .cloned()
            .collect();
        for ((network, scope_path, node_id), name) in frozen_nodes_of(registry, &locals) {
            report.frozen_nodes.push(library_refresh::ReportedNode {
                network,
                scope_path,
                node_id,
                name,
            });
        }
        for m in registry.library_links.direct_mounts() {
            if m.status != MountStatus::Loaded {
                report
                    .status_changes
                    .push((m.mount_path.clone(), m.status.clone()));
            }
            if let (Some(stored), Some(loaded)) = (&m.stored_hash, &m.loaded)
                && *stored != loaded.hash_string()
            {
                report.changed_since_saved.push(m.mount_path.clone());
            }
        }
        let reconciled = !report.reconciled_nodes.is_empty();
        registry.library_links.load_report = report;
        if reconciled {
            self.is_dirty = true;
        }
    }

    /// Clears a stale "changed on disk" / "older than disk" marker on mounts
    /// and held data files that turn out to match the disk after all.
    fn settle_unchanged(&mut self, aliases: &[String], busy: &BTreeSet<PathBuf>) -> RefreshReport {
        let mut report = RefreshReport::default();
        let registry = &mut self.node_type_registry;
        let fs = registry.library_links.fs();
        for alias in aliases {
            let paths: Vec<String> = registry
                .library_links
                .iter()
                .filter(|m| library_links::is_under(&m.mount_path, alias))
                .map(|m| m.mount_path.clone())
                .collect();
            for p in paths {
                let Some(m) = registry.library_links.get_mut(&p) else {
                    continue;
                };
                if m.loaded.is_some()
                    && matches!(
                        m.status,
                        MountStatus::ChangedOnDisk | MountStatus::OlderThanDisk
                    )
                {
                    m.status = MountStatus::Loaded;
                    m.last_seen = stamp_file(fs.as_ref(), &m.abs_path).or(m.last_seen.take());
                    report.status_changes.push((p, MountStatus::Loaded));
                }
            }
        }
        for (key, watch) in registry.library_links.data_files.iter_mut() {
            if key.owner.is_none() && !busy.contains(&key.path) {
                watch.held = false;
            }
        }
        report
    }

    /// §7.1: refreshes the direct mounts `targets` (retargeting those with a
    /// new path) and re-reads the host data files `host_files`, as **one**
    /// undo step. A target whose file cannot be mounted keeps its loaded
    /// content and only changes status (D10) — or, for a retarget, fails the
    /// whole operation with nothing changed.
    pub fn refresh_dependencies(
        &mut self,
        targets: &[RefreshTarget],
        host_files: &BTreeSet<PathBuf>,
    ) -> Result<RefreshReport, String> {
        if let Some(what) = self.open_interaction() {
            return Err(format!("finish {} before refreshing libraries", what));
        }
        let mut report = RefreshReport::default();
        let aliases: Vec<String> = targets.iter().map(|t| t.alias.clone()).collect();
        let retarget = targets.iter().any(|t| t.new_rel_path.is_some());

        // Step 2: every host network that refers to an affected mount, or
        // reads an affected data file, before anything touches it.
        let registry = &self.node_type_registry;
        let host_dir = registry
            .design_file_name
            .as_ref()
            .and_then(|p| Path::new(p).parent().map(Path::to_path_buf));
        let mut affected: BTreeSet<String> = local_networks_referring_to(registry, &aliases)
            .into_iter()
            .collect();
        if !host_files.is_empty() {
            for (name, network) in &registry.node_networks {
                if registry.library_links.mount_containing(name).is_some() {
                    continue;
                }
                let mut reads = false;
                walk_all_nodes(network, &mut |node| {
                    reads |= node.data.file_paths().iter().any(|p| {
                        library_refresh::resolve_data_path(host_dir.as_deref(), p)
                            .is_some_and(|p| host_files.contains(&p))
                    });
                });
                if reads {
                    affected.insert(name.clone());
                }
            }
        }
        let affected: Vec<String> = affected.into_iter().collect();
        let host_before: Vec<(String, NodeNetwork)> = affected
            .iter()
            .map(|n| (n.clone(), registry.node_networks[n].clone()))
            .collect();
        let frozen_before = frozen_nodes_of(registry, &affected);
        let names_before = referenced_linked_names(registry, &aliases);
        let data_before = registry.library_links.data_files.clone();
        let linked_before: Vec<String> = registry
            .node_networks
            .keys()
            .filter(|n| aliases.iter().any(|a| library_links::is_under(n, a)))
            .cloned()
            .collect();

        // Steps 1, 3, 4: capture the interfaces the host is wired against,
        // detach the old content (kept for undo), mount again.
        let mut swaps: Vec<(String, library_links::DetachedMount)> = Vec::new();
        for target in targets {
            let registry = &mut self.node_type_registry;
            let Some(record) = registry.library_links.get(&target.alias).cloned() else {
                report
                    .errors
                    .push(format!("no linked library '{}'", target.alias));
                continue;
            };
            let captured = library_links::collect_used_interfaces(registry, &target.alias);
            let before = library_links::unmount(registry, &target.alias);
            let spec = ImportSpec {
                alias: target.alias.clone(),
                rel_path: target
                    .new_rel_path
                    .clone()
                    .unwrap_or_else(|| record.rel_path.clone()),
                stored_hash: record.stored_hash.clone(),
                stored_uses: captured,
            };
            let status = mount_direct(registry, &spec);
            if status != MountStatus::Loaded {
                // Not a refresh (D10): the loaded content keeps evaluating,
                // the mount shows why.
                let seen = registry
                    .library_links
                    .get(&target.alias)
                    .and_then(|m| m.last_seen.clone());
                library_links::unmount(registry, &target.alias);
                library_links::restore_detached(registry, &before);
                if target.new_rel_path.is_some() {
                    return Err(format!(
                        "cannot link '{}': {}",
                        spec.rel_path,
                        status.message()
                    ));
                }
                if let Some(m) = registry.library_links.get_mut(&target.alias) {
                    m.last_seen = seen;
                    if m.status != status {
                        m.status = status.clone();
                        report.status_changes.push((target.alias.clone(), status));
                    }
                }
                continue;
            }
            report
                .refreshed_mounts
                .push(format!("{} ({})", target.alias, spec.rel_path));
            if record.status != MountStatus::Loaded {
                report
                    .status_changes
                    .push((target.alias.clone(), MountStatus::Loaded));
            }
            swaps.push((target.alias.clone(), before));
        }

        // Host data files (§7.1 step 4): re-read, every user field kept.
        let registry = &mut self.node_type_registry;
        let fs = registry.library_links.fs();
        if !host_files.is_empty() {
            for name in &affected {
                if let Some(mut network) = registry.node_networks.remove(name) {
                    reload_data_file_nodes(&mut network, registry, host_dir.as_deref(), host_files);
                    registry.node_networks.insert(name.clone(), network);
                }
            }
            for path in host_files {
                let key = DataFileKey {
                    owner: None,
                    path: path.clone(),
                };
                let stamp = stamp_file(fs.as_ref(), path);
                if let Some(w) = registry.library_links.data_files.get_mut(&key) {
                    w.loaded = stamp.clone();
                    w.last_seen = stamp;
                    w.held = false;
                }
                report
                    .refreshed_data_files
                    .push(path.to_string_lossy().to_string());
            }
        }
        if swaps.is_empty() && host_files.is_empty() {
            return Ok(report);
        }

        // Step 5: move the host's wiring to the new interfaces; step 7:
        // validate everything in dependency order.
        let mut images = Vec::new();
        for name in &affected {
            if let Some(mut network) = registry.node_networks.remove(name) {
                library_refresh::reconcile_network(&mut network, registry, None, name, &mut report);
                images.push((
                    name.clone(),
                    library_refresh::wire_images(&network, registry, name),
                ));
                registry.repair_node_network(&mut network);
                registry.node_networks.insert(name.clone(), network);
            }
        }
        library_links::validate_all_networks(registry);
        // Whatever the repair and validation passes removed on top of the
        // reconciliation is reported too (§7.2).
        for (name, before) in &images {
            if let Some(network) = registry.node_networks.get(name) {
                library_refresh::report_wires_removed_by_repair(
                    before,
                    network,
                    registry,
                    name,
                    &mut report,
                );
            }
        }

        // Step 6: what froze, what disappeared.
        for (key, name) in frozen_nodes_of(registry, &affected) {
            if !frozen_before.contains_key(&key) {
                let (network, scope_path, node_id) = key;
                report.frozen_nodes.push(library_refresh::ReportedNode {
                    network,
                    scope_path,
                    node_id,
                    name,
                });
            }
        }
        let names_after = referenced_linked_names(registry, &aliases);
        for (name, resolved) in names_before {
            if resolved && names_after.get(&name) == Some(&false) {
                report.removed_names_in_use.push(name);
            }
        }

        rebuild_data_watches(registry);
        let mounts: Vec<MountSwap> = swaps
            .into_iter()
            .map(|(alias, before)| {
                let after = snapshot_mount(registry, &alias);
                MountSwap {
                    alias,
                    before,
                    after,
                }
            })
            .collect();
        let host_after: Vec<(String, NodeNetwork)> = affected
            .iter()
            .filter_map(|n| {
                registry
                    .node_networks
                    .get(n)
                    .map(|net| (n.clone(), net.clone()))
            })
            .collect();
        let data_after: BTreeMap<_, _> = registry.library_links.data_files.clone();

        // The editor falls back to a local network when the one it shows, or
        // one in its history, was removed.
        if self
            .active_node_network_name
            .as_ref()
            .is_some_and(|n| !self.node_type_registry.node_networks.contains_key(n))
        {
            let mut local: Vec<&String> = self
                .node_type_registry
                .node_networks
                .keys()
                .filter(|n| {
                    self.node_type_registry
                        .library_links
                        .mount_containing(n)
                        .is_none()
                })
                .collect();
            local.sort();
            let fallback = local.first().map(|s| (*s).clone());
            self.set_active_node_network_name(fallback);
        }
        for name in linked_before {
            if !self.node_type_registry.node_networks.contains_key(&name) {
                self.navigation_history.remove_network(&name);
            }
        }

        let description = if retarget {
            format!("Change file of {}", aliases.join(", "))
        } else if mounts.is_empty() {
            "Refresh data files".to_string()
        } else {
            format!(
                "Refresh {}",
                mounts
                    .iter()
                    .map(|m| m.alias.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        self.push_command(RefreshDependenciesCommand {
            description,
            mounts,
            host_before,
            host_after,
            data_before,
            data_after,
        });
        self.set_dirty(true);
        self.apply_node_display_policy(None);
        self.mark_full_refresh();
        Ok(report)
    }
}
