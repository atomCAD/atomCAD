//! Undo commands for linking and unlinking a library
//! (`doc/design_library_linking.md` D9), and for the two operations that
//! change what a link *is*: *Make local copy* (§10) and *Rename alias…* (D3).
//!
//! Both are inverses of each other: one side mounts the library from disk, the
//! other unmounts it. **Mounting reads the disk**, so a redo of *Link* (and an
//! undo of *Unlink*) brings in the library's *current* content, not the content
//! it had when the command was recorded. That is acceptable because unlinking
//! is only allowed while nothing local refers to the mount, so no local wiring
//! depends on the content that was there.

use crate::library_links::{self, ImportSpec, LibraryMount, is_under};
use crate::node_type_registry::NodeTypeRegistry;
use crate::undo::{UndoCommand, UndoContext, UndoRefreshMode};
use std::path::PathBuf;

/// Mounts `spec` as a direct link of the registry's design file and
/// revalidates everything (names under the new mount may now resolve).
pub(crate) fn remount(registry: &mut NodeTypeRegistry, spec: &ImportSpec) {
    let Some(host) = registry.design_file_name.clone() else {
        return;
    };
    let fs = registry.library_links.fs();
    let host_path = PathBuf::from(&host);
    let canonical = fs.canonicalize(&host_path).unwrap_or(host_path.clone());
    let mut stack = vec![canonical];
    library_links::mount_library(registry, spec, &host_path, &mut stack);
    library_links::validate_all_networks(registry);
    crate::library_refresh::rebuild_data_watches(registry);
}

/// Removes the mount and everything under it, moves the active network off
/// it, and revalidates.
pub(crate) fn detach(
    registry: &mut NodeTypeRegistry,
    active_network_name: &mut Option<String>,
    alias: &str,
) {
    library_links::unmount(registry, alias);
    if active_network_name
        .as_deref()
        .is_some_and(|name| is_under(name, alias))
    {
        let mut local: Vec<&String> = registry
            .node_networks
            .keys()
            .filter(|n| registry.library_links.mount_containing(n).is_none())
            .collect();
        local.sort();
        *active_network_name = local.first().map(|s| (*s).clone());
    }
    library_links::validate_all_networks(registry);
    crate::library_refresh::rebuild_data_watches(registry);
}

fn spec_of(mount: &LibraryMount) -> ImportSpec {
    ImportSpec {
        alias: mount.alias.clone(),
        rel_path: mount.rel_path.clone(),
        stored_hash: mount.stored_hash.clone(),
        stored_uses: mount.stored_uses.clone(),
    }
}

/// *Link library…*: undo unmounts, redo mounts from disk again.
#[derive(Debug)]
pub struct LinkLibraryCommand {
    pub mount: LibraryMount,
}

impl UndoCommand for LinkLibraryCommand {
    fn description(&self) -> &str {
        "Link library"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        detach(
            ctx.node_type_registry,
            ctx.active_network_name,
            &self.mount.alias,
        );
    }

    fn redo(&self, ctx: &mut UndoContext) {
        remount(ctx.node_type_registry, &spec_of(&self.mount));
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}

/// *Unlink*: undo mounts from disk again, redo unmounts.
#[derive(Debug)]
pub struct UnlinkLibraryCommand {
    pub mount: LibraryMount,
}

impl UndoCommand for UnlinkLibraryCommand {
    fn description(&self) -> &str {
        "Unlink library"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        remount(ctx.node_type_registry, &spec_of(&self.mount));
    }

    fn redo(&self, ctx: &mut UndoContext) {
        detach(
            ctx.node_type_registry,
            ctx.active_network_name,
            &self.mount.alias,
        );
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}

// ---------------------------------------------------------------------------
// Make local copy (vendoring, §10)
// ---------------------------------------------------------------------------

/// Watches of `previous` carried over to the keys the registry wants now:
/// a data file whose key changed but which is the same file on disk (its
/// owner changed, or its stored path was rebased) keeps its `loaded` /
/// `last_seen` stamps and its hold, so a change that was detected — or held —
/// before stays detected (D7, D9).
pub(crate) fn rebuild_watches_carrying(
    registry: &mut NodeTypeRegistry,
    previous: &std::collections::BTreeMap<
        crate::library_refresh::DataFileKey,
        crate::library_refresh::DataFileWatch,
    >,
) {
    let fs = registry.library_links.fs();
    let canonical = |p: &std::path::Path| fs.canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let wanted = crate::library_refresh::watched_data_files(registry);
    let mut watches = std::collections::BTreeMap::new();
    for key in wanted {
        let carried = previous.get(&key).cloned().or_else(|| {
            let target = canonical(&key.path);
            previous
                .iter()
                .find(|(k, _)| canonical(&k.path) == target)
                .map(|(_, w)| w.clone())
        });
        if let Some(watch) = carried {
            watches.insert(key, watch);
        }
    }
    registry.library_links.data_files = watches;
    // Whatever had no predecessor is stamped fresh.
    crate::library_refresh::rebuild_data_watches(registry);
}

/// *Make local copy*: the mount records of a direct mount (and its nested
/// ones) are removed, so its content — already in the registry under the
/// alias — becomes the design's own and is saved from now on. Networks whose
/// stored data-file paths had to be rebased onto the design's folder are kept
/// in both versions. Undo puts the mount records and the library-relative
/// paths back; nothing is read from disk either way.
///
/// Undo also puts back the file's id floors (`param_id_floor` /
/// `field_id_floor`): a save while the content was local raises them to
/// cover its ids, and once it is linked again those ids belong to the
/// library's history, not the design's.
pub struct VendorLibraryCommand {
    pub alias: String,
    pub mounts: Vec<LibraryMount>,
    pub id_floors: (u64, u64),
    pub networks_before: Vec<(String, crate::node_network::NodeNetwork)>,
    pub networks_after: Vec<(String, crate::node_network::NodeNetwork)>,
}

impl std::fmt::Debug for VendorLibraryCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VendorLibraryCommand")
            .field("alias", &self.alias)
            .field(
                "mounts",
                &self
                    .mounts
                    .iter()
                    .map(|m| &m.mount_path)
                    .collect::<Vec<_>>(),
            )
            .field(
                "rebased",
                &self
                    .networks_before
                    .iter()
                    .map(|(n, _)| n)
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl UndoCommand for VendorLibraryCommand {
    fn description(&self) -> &str {
        "Make library local"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        let registry = &mut *ctx.node_type_registry;
        let previous = registry.library_links.data_files.clone();
        for m in &self.mounts {
            registry.library_links.insert(m.clone());
        }
        // A Save As since may have moved the design's folder (D11).
        library_links::relocate_mounts(registry);
        for (name, network) in &self.networks_before {
            registry.node_networks.insert(name.clone(), network.clone());
        }
        (registry.param_id_floor, registry.field_id_floor) = self.id_floors;
        rebuild_watches_carrying(registry, &previous);
        library_links::validate_all_networks(registry);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        let registry = &mut *ctx.node_type_registry;
        let previous = registry.library_links.data_files.clone();
        registry.library_links.remove_subtree(&self.alias);
        for (name, network) in &self.networks_after {
            registry.node_networks.insert(name.clone(), network.clone());
        }
        rebuild_watches_carrying(registry, &previous);
        library_links::validate_all_networks(registry);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}

// ---------------------------------------------------------------------------
// Rename alias (D3)
// ---------------------------------------------------------------------------

/// Moves every name under `from` to `to` (`library_links::rename_prefix`)
/// together with the per-network state an undo context can reach: the active
/// network and record def, and the eval-error snapshots. Revalidates, so that
/// error texts spelling the old names are rebuilt.
pub(crate) fn apply_alias_rename(
    registry: &mut NodeTypeRegistry,
    active_network_name: &mut Option<String>,
    active_record_def_name: &mut Option<String>,
    eval_error_snapshots: &mut std::collections::HashMap<
        String,
        Vec<crate::eval_errors::EvalErrorEntry>,
    >,
    from: &str,
    to: &str,
) {
    let renamed: Vec<String> = registry
        .node_networks
        .keys()
        .filter(|k| is_under(k, from))
        .cloned()
        .collect();
    library_links::rename_prefix(registry, from, to);
    for old in &renamed {
        let new = library_links::reprefixed(old, from, to).expect("filtered above");
        if let Some(entries) = eval_error_snapshots.remove(old) {
            eval_error_snapshots.insert(new.clone(), entries);
        }
        crate::eval_errors::rewrite_network_name_in_snapshots(eval_error_snapshots, old, &new);
    }
    for name in [active_network_name, active_record_def_name] {
        if let Some(new) = name
            .as_deref()
            .and_then(|n| library_links::reprefixed(n, from, to))
        {
            *name = Some(new);
        }
    }
    library_links::validate_all_networks(registry);
}

/// *Rename alias…*: a pure rename, so undo is the rename back.
#[derive(Debug)]
pub struct RenameLibraryAliasCommand {
    pub old_alias: String,
    pub new_alias: String,
}

impl RenameLibraryAliasCommand {
    fn apply(&self, ctx: &mut UndoContext, from: &str, to: &str) {
        apply_alias_rename(
            ctx.node_type_registry,
            ctx.active_network_name,
            ctx.active_record_def_name,
            ctx.eval_error_snapshots,
            from,
            to,
        );
        ctx.clipboard.reprefix(from, to);
    }
}

impl UndoCommand for RenameLibraryAliasCommand {
    fn description(&self) -> &str {
        "Rename library alias"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, &self.new_alias, &self.old_alias);
    }

    fn redo(&self, ctx: &mut UndoContext) {
        self.apply(ctx, &self.old_alias, &self.new_alias);
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
