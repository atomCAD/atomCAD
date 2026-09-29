//! Undo command for a refresh or a retarget of linked libraries and data
//! files (`doc/design_library_linking.md` D9, §7.1).
//!
//! The command keeps both versions of everything the refresh touched, in
//! memory: the content that was mounted before and after (it was already in
//! memory — it is kept instead of dropped), the mount records (so a retarget's
//! undo points the alias back at the old file), the data-file watch state, and
//! whole copies of every host network that referred to a refreshed mount or
//! read a refreshed data file, taken before the refresh and after its
//! validation. Undo and redo swap them wholesale, so the round trip is exact
//! whatever the repair and validation passes did in between.
//!
//! **Live copies, not serialized snapshots.** Restoring a serialized network
//! runs the node-data loaders, which re-read data files from disk — an undo of
//! a data-file refresh would read the *new* file. That is also why
//! `UndoCommand` carries no `Send` / `Sync` bound.
//!
//! After an undo, memory is older than the disk: the stamps keep what the disk
//! was last seen as (`last_seen`), the affected mounts get `OlderThanDisk`, and
//! by D7's rule no automatic refresh follows.

use crate::library_links::{self, DetachedMount, MountStatus};
use crate::library_refresh::{DataFileKey, DataFileWatch};
use crate::node_network::NodeNetwork;
use crate::node_type_registry::NodeTypeRegistry;
use crate::undo::{UndoCommand, UndoContext, UndoRefreshMode};
use std::collections::BTreeMap;

/// The content of one direct mount before and after the refresh.
pub struct MountSwap {
    pub alias: String,
    pub before: DetachedMount,
    pub after: DetachedMount,
}

/// One refresh / retarget (D9): one undo step.
pub struct RefreshDependenciesCommand {
    pub description: String,
    pub mounts: Vec<MountSwap>,
    /// Host networks as they were before the refresh…
    pub host_before: Vec<(String, NodeNetwork)>,
    /// …and after it, validated.
    pub host_after: Vec<(String, NodeNetwork)>,
    pub data_before: BTreeMap<DataFileKey, DataFileWatch>,
    pub data_after: BTreeMap<DataFileKey, DataFileWatch>,
}

impl std::fmt::Debug for RefreshDependenciesCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RefreshDependenciesCommand")
            .field("description", &self.description)
            .field(
                "mounts",
                &self.mounts.iter().map(|m| &m.alias).collect::<Vec<_>>(),
            )
            .field(
                "host",
                &self.host_before.iter().map(|(n, _)| n).collect::<Vec<_>>(),
            )
            .field("data_files", &self.data_before.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// Installs one side of the command. `last_seen` stamps (the disk as last
/// observed) are carried over from the current state rather than taken from
/// the snapshot, and a mount whose content in memory differs from the disk so
/// observed is marked `OlderThanDisk` (D9).
fn install(
    registry: &mut NodeTypeRegistry,
    active_network_name: &mut Option<String>,
    mounts: &[(&str, &DetachedMount)],
    host: &[(String, NodeNetwork)],
    data: &BTreeMap<DataFileKey, DataFileWatch>,
) {
    for (alias, content) in mounts {
        let seen: BTreeMap<String, (std::path::PathBuf, Option<library_links::FileStamp>)> =
            registry
                .library_links
                .iter()
                .filter(|m| library_links::is_under(&m.mount_path, alias))
                .map(|m| {
                    (
                        m.mount_path.clone(),
                        (m.abs_path.clone(), m.last_seen.clone()),
                    )
                })
                .collect();
        library_links::unmount(registry, alias);
        library_links::restore_detached(registry, content);
        // The records were resolved against the design's folder at the time;
        // a Save As since may have moved it (D11).
        library_links::relocate_mounts(registry);
        for m in &content.mounts {
            let Some((abs_path, last_seen)) = seen.get(&m.mount_path) else {
                continue;
            };
            // A retarget changes the file: its last sighting says nothing
            // about the other one.
            if *abs_path != m.abs_path {
                continue;
            }
            if let Some(record) = registry.library_links.get_mut(&m.mount_path) {
                record.last_seen = last_seen.clone();
                let loaded = record.loaded.as_ref().map(|s| &s.blake3);
                let disk = last_seen.as_ref().map(|s| &s.blake3);
                if disk.is_some() && loaded != disk {
                    record.status = MountStatus::OlderThanDisk;
                }
            }
        }
    }
    for (name, network) in host {
        registry.node_networks.insert(name.clone(), network.clone());
    }
    let mut watches = data.clone();
    for (key, watch) in watches.iter_mut() {
        if let Some(current) = registry.library_links.data_files.get(key) {
            watch.last_seen = current.last_seen.clone();
        }
    }
    registry.library_links.data_files = watches;
    // Likewise for the data files: a watch keyed by a path from before a
    // Save As is dropped, and the path the node reads now is stamped as seen.
    crate::library_refresh::rebuild_data_watches(registry);
    // The active network may have been one the other side does not have.
    if active_network_name
        .as_ref()
        .is_some_and(|n| !registry.node_networks.contains_key(n))
    {
        let mut local: Vec<&String> = registry
            .node_networks
            .keys()
            .filter(|n| registry.library_links.mount_containing(n).is_none())
            .collect();
        local.sort();
        *active_network_name = local.first().map(|s| (*s).clone());
    }
}

impl UndoCommand for RefreshDependenciesCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, ctx: &mut UndoContext) {
        let mounts: Vec<(&str, &DetachedMount)> = self
            .mounts
            .iter()
            .map(|m| (m.alias.as_str(), &m.before))
            .collect();
        install(
            ctx.node_type_registry,
            ctx.active_network_name,
            &mounts,
            &self.host_before,
            &self.data_before,
        );
    }

    fn redo(&self, ctx: &mut UndoContext) {
        let mounts: Vec<(&str, &DetachedMount)> = self
            .mounts
            .iter()
            .map(|m| (m.alias.as_str(), &m.after))
            .collect();
        install(
            ctx.node_type_registry,
            ctx.active_network_name,
            &mounts,
            &self.host_after,
            &self.data_after,
        );
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
