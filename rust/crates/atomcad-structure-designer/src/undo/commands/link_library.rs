//! Undo commands for linking and unlinking a library
//! (`doc/design_library_linking.md` D9).
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
