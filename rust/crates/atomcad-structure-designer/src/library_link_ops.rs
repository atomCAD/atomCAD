//! *Link library…* and *Unlink*: the two `StructureDesigner` operations of
//! library linking Phase 1 (`doc/design_library_linking.md` §3, D2/D3/D6/D9).
//!
//! Rust owns every decision here — alias validity, path normalization, the
//! "mount owns its folder" rule, the unlink users check — so a missed Flutter
//! gate produces an error message, never corrupted state (§5).

use crate::library_links::{
    self, ImportSpec, LibraryMount, MountStatus, UsedInterfaces, is_under, normalize_rel_path,
    relative_path, validate_alias,
};
use crate::node_network::{NodeNetwork, walk_all_nodes_mut};
use crate::structure_designer::StructureDesigner;
use crate::undo::commands::link_library::{
    LinkLibraryCommand, RenameLibraryAliasCommand, UnlinkLibraryCommand, VendorLibraryCommand,
};
use std::path::{Path, PathBuf};

/// Whole copies of networks, by name — one side of a command's before/after.
type NetworkCopies = Vec<(String, NodeNetwork)>;

impl StructureDesigner {
    /// The mounts of the open design, direct and nested.
    pub fn linked_libraries(&self) -> Vec<LibraryMount> {
        self.node_type_registry
            .library_links
            .iter()
            .cloned()
            .collect()
    }

    /// Why `alias` cannot be linked as a new direct mount, if it cannot (D3):
    /// an invalid segment; equal to, containing or contained in an existing
    /// mount; equal to or containing a local name (entity, folder marker or
    /// built-in); lying under a local *entity*. Lying under a local folder is
    /// fine — the mount then appears inside it.
    pub fn check_new_alias(&self, alias: &str) -> Result<(), String> {
        validate_alias(alias)?;
        let registry = &self.node_type_registry;
        if let Some(m) = registry
            .library_links
            .iter()
            .find(|m| is_under(alias, &m.mount_path) || is_under(&m.mount_path, alias))
        {
            return Err(format!(
                "alias '{}' overlaps the linked library '{}'",
                alias, m.mount_path
            ));
        }
        let local_names = registry
            .node_networks
            .keys()
            .chain(registry.record_type_defs.keys())
            .chain(registry.built_in_node_types.keys())
            .chain(registry.built_in_record_type_defs.keys())
            .chain(registry.folders.iter());
        for name in local_names {
            if is_under(name, alias) {
                return Err(format!("alias '{}' is taken by '{}'", alias, name));
            }
        }
        let entity_names = registry
            .node_networks
            .keys()
            .chain(registry.record_type_defs.keys())
            .chain(registry.built_in_node_types.keys())
            .chain(registry.built_in_record_type_defs.keys());
        for name in entity_names {
            if is_under(alias, name) {
                return Err(format!(
                    "alias '{}' would lie inside '{}', which is not a folder",
                    alias, name
                ));
            }
        }
        Ok(())
    }

    /// The design file's path, which every library path is relative to.
    pub(crate) fn host_file(&self) -> Option<PathBuf> {
        self.file_path
            .clone()
            .or_else(|| self.node_type_registry.design_file_name.clone())
            .map(PathBuf::from)
    }

    /// Turns what the user picked into the path stored in the host (D6): a
    /// relative path is normalized lexically; an absolute one is made relative
    /// to the host's folder, and refused when no relative path exists
    /// (another drive).
    pub(crate) fn library_rel_path(&self, host: &Path, path: &str) -> Result<String, String> {
        let fs = self.node_type_registry.library_links.fs();
        let picked = Path::new(path);
        let rel = if picked.is_absolute() {
            let host_dir = host.parent().unwrap_or(Path::new(""));
            let host_dir = fs
                .canonicalize(host_dir)
                .map_err(|e| format!("cannot resolve the design's folder: {}", e))?;
            let target = fs
                .canonicalize(picked)
                .map_err(|e| format!("cannot open '{}': {}", path, e))?;
            relative_path(&host_dir, &target).ok_or_else(|| {
                format!(
                    "'{}' has no path relative to the design's folder (another drive?); \
                     copy it next to the design to link it",
                    path
                )
            })?
        } else {
            path.to_string()
        };
        let rel = normalize_rel_path(&rel)?;
        if !rel.to_ascii_lowercase().ends_with(".cnnd") {
            return Err(format!("'{}' is not a .cnnd file", rel));
        }
        // Refuse a path that climbs above the filesystem root.
        library_links::lexical_join(host.parent().unwrap_or(Path::new("")), &rel)?;
        Ok(rel)
    }

    /// *File > Link library…*: mounts the `.cnnd` at `path` (relative to the
    /// design's folder, or absolute) under `alias`. One undo step. On any
    /// failure — unsaved design, bad alias, bad path, unreadable or unparseable
    /// file, cycle — nothing is mounted and the undo stack is unchanged.
    pub fn link_library(&mut self, path: &str, alias: &str) -> Result<(), String> {
        let host = self
            .host_file()
            .ok_or_else(|| "save the design before linking a library".to_string())?;
        self.check_new_alias(alias)?;
        let rel_path = self.library_rel_path(&host, path)?;

        let spec = ImportSpec {
            alias: alias.to_string(),
            rel_path,
            stored_hash: None,
            stored_uses: UsedInterfaces::default(),
        };
        let fs = self.node_type_registry.library_links.fs();
        let canonical_host = fs.canonicalize(&host).unwrap_or(host.clone());
        let mut stack = vec![canonical_host];
        let status =
            library_links::mount_library(&mut self.node_type_registry, &spec, &host, &mut stack);
        if status != MountStatus::Loaded {
            // `mount_library` put only the status record in; take it out again.
            library_links::unmount(&mut self.node_type_registry, alias);
            return Err(format!(
                "cannot link '{}': {}",
                spec.rel_path,
                status.message()
            ));
        }
        let mount = self
            .node_type_registry
            .library_links
            .get(alias)
            .cloned()
            .expect("just mounted");
        library_links::validate_all_networks(&mut self.node_type_registry);
        crate::library_refresh::rebuild_data_watches(&mut self.node_type_registry);
        self.push_command(LinkLibraryCommand { mount });
        self.set_dirty(true);
        self.apply_node_display_policy(None);
        self.mark_full_refresh();
        Ok(())
    }

    /// *Unlink*: removes the direct mount `alias`. Refused, with the list of
    /// users, while anything local refers to the mount by any reference kind —
    /// so unlinking can never freeze a working node (D2). One undo step.
    pub fn unlink_library(&mut self, alias: &str) -> Result<(), String> {
        let mount = match self.node_type_registry.library_links.get(alias) {
            Some(m) if m.is_direct() => m.clone(),
            Some(_) => {
                return Err(format!(
                    "'{}' is linked by another library; only direct links can be unlinked",
                    alias
                ));
            }
            None => return Err(format!("no linked library '{}'", alias)),
        };
        let users = library_links::mount_users(&self.node_type_registry, alias);
        if !users.is_empty() {
            return Err(format!(
                "'{}' is still used by: {}",
                alias,
                users.join(", ")
            ));
        }
        let mut active = self.active_node_network_name.clone();
        crate::undo::commands::link_library::detach(
            &mut self.node_type_registry,
            &mut active,
            alias,
        );
        if active != self.active_node_network_name {
            self.set_active_node_network_name(active);
        }
        self.push_command(UnlinkLibraryCommand { mount });
        self.set_dirty(true);
        self.mark_full_refresh();
        Ok(())
    }

    /// *Make local copy* on a mount folder (§10, "vendoring"): the direct
    /// mount `alias` — nested mounts included — stops being a link, and its
    /// content, as it is **in memory**, becomes the design's own: saved from
    /// now on, editable, no `imports` entry. One undo step.
    ///
    /// Refused unless every mount in the subtree has content (there is
    /// nothing to copy from a missing file), and while anything refers to a
    /// name under the mount that does not resolve: such a node is safe only
    /// while the name lies under a mount (§8). Relative data-file paths of the
    /// library's nodes are rebased onto the design's folder so they keep
    /// naming the same files (a path that arrives through a wire cannot be,
    /// D8).
    pub fn make_library_local(&mut self, alias: &str) -> Result<(), String> {
        match self.node_type_registry.library_links.get(alias) {
            Some(m) if m.is_direct() => {}
            Some(_) => {
                return Err(format!(
                    "'{}' is linked by another library; only a direct link can be made local",
                    alias
                ));
            }
            None => return Err(format!("no linked library '{}'", alias)),
        }
        let registry = &self.node_type_registry;
        if let Some(m) = registry
            .library_links
            .iter()
            .find(|m| is_under(&m.mount_path, alias) && !m.status.has_content())
        {
            return Err(format!(
                "cannot make '{}' local: '{}' ({}) is not loaded — {}",
                alias,
                m.mount_path,
                m.rel_path,
                m.status.message()
            ));
        }
        let unresolved = library_links::unresolved_refs_under(registry, alias);
        if !unresolved.is_empty() {
            return Err(format!(
                "cannot make '{}' local: these refer to names the library does not define: {}",
                alias,
                unresolved.join(", ")
            ));
        }

        let (networks_before, networks_after) = self.rebased_library_networks(alias);
        let registry = &mut self.node_type_registry;
        let id_floors = (registry.param_id_floor, registry.field_id_floor);
        let previous = registry.library_links.data_files.clone();
        let mounts = registry.library_links.remove_subtree(alias);
        for (name, network) in &networks_after {
            registry.node_networks.insert(name.clone(), network.clone());
        }
        crate::undo::commands::link_library::rebuild_watches_carrying(registry, &previous);
        library_links::validate_all_networks(registry);
        self.push_command(VendorLibraryCommand {
            alias: alias.to_string(),
            mounts,
            id_floors,
            networks_before,
            networks_after,
        });
        self.set_dirty(true);
        self.mark_full_refresh();
        Ok(())
    }

    /// The networks under `alias` whose stored data-file paths change when
    /// they become the design's own, as `(before, after)` copies: a relative
    /// path authored against a library's folder is re-spelled against the
    /// design's folder (absolute when no relative path exists — another
    /// drive). Networks in the design's own folder need nothing.
    fn rebased_library_networks(&self, alias: &str) -> (NetworkCopies, NetworkCopies) {
        let registry = &self.node_type_registry;
        let fs = registry.library_links.fs();
        let canonical = |p: &Path| fs.canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let Some(host_dir) = self
            .host_file()
            .and_then(|h| h.parent().map(Path::to_path_buf))
            .map(|d| canonical(&d))
        else {
            return (Vec::new(), Vec::new());
        };
        let mut names: Vec<&String> = registry
            .node_networks
            .keys()
            .filter(|k| is_under(k, alias))
            .collect();
        names.sort();
        let (mut before, mut after) = (Vec::new(), Vec::new());
        for name in names {
            let Some(lib_dir) = crate::library_refresh::base_dir_of(registry, name) else {
                continue;
            };
            let lib_dir = canonical(&lib_dir);
            if lib_dir == host_dir {
                continue;
            }
            let rebase = |stored: &str| -> Option<String> {
                if stored.is_empty() || Path::new(stored).is_absolute() {
                    return None;
                }
                let target = library_links::lexical_join(&lib_dir, stored).ok()?;
                let spelled = relative_path(&host_dir, &target).unwrap_or_else(|| {
                    let s = target.to_string_lossy().to_string();
                    s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s)
                });
                (spelled != stored).then_some(spelled)
            };
            let original = &registry.node_networks[name];
            let mut copy = original.clone();
            let changed = std::cell::Cell::new(false);
            let tracked = |stored: &str| {
                let r = rebase(stored);
                changed.set(changed.get() || r.is_some());
                r
            };
            walk_all_nodes_mut(&mut copy, &mut |node| {
                node.data.rebase_file_paths(&tracked);
            });
            if changed.get() {
                before.push((name.clone(), original.clone()));
                after.push((name.clone(), copy));
            }
        }
        (before, after)
    }

    /// *Rename alias…* on a direct mount (D3): every name under `alias` —
    /// the library's networks, record defs and folders, its nested mounts, and
    /// every reference to them in the design, including those of frozen nodes
    /// and the recorded interfaces — moves under `new_alias` in one undoable
    /// step. The library file is untouched; the next save writes the new alias.
    pub fn rename_library_alias(&mut self, alias: &str, new_alias: &str) -> Result<(), String> {
        match self.node_type_registry.library_links.get(alias) {
            Some(m) if m.is_direct() => {}
            Some(_) => {
                return Err(format!(
                    "'{}' is linked by another library; only a direct link's alias can be renamed",
                    alias
                ));
            }
            None => return Err(format!("no linked library '{}'", alias)),
        }
        if new_alias == alias {
            return Ok(());
        }
        validate_alias(new_alias)?;
        if is_under(new_alias, alias) || is_under(alias, new_alias) {
            return Err(format!(
                "'{}' contains or lies inside '{}'; rename in two steps through another alias",
                new_alias, alias
            ));
        }
        self.check_new_alias(new_alias)?;
        // Nothing may already refer to a name under the new alias (a dangling
        // reference would silently start resolving into the library).
        let users = library_links::mount_users(&self.node_type_registry, new_alias);
        if !users.is_empty() {
            return Err(format!(
                "names under '{}' are already referred to by: {}",
                new_alias,
                users.join(", ")
            ));
        }
        let renamed: Vec<String> = self
            .node_type_registry
            .node_networks
            .keys()
            .filter(|k| is_under(k, alias))
            .cloned()
            .collect();
        crate::undo::commands::link_library::apply_alias_rename(
            &mut self.node_type_registry,
            &mut self.active_node_network_name,
            &mut self.active_record_def_name,
            &mut self.eval_error_snapshots,
            alias,
            new_alias,
        );
        for old in &renamed {
            if let Some(new) = library_links::reprefixed(old, alias, new_alias) {
                self.navigation_history.rename_network(old, &new);
            }
        }
        if let Some(clipboard) = self.clipboard.as_mut() {
            walk_all_nodes_mut(clipboard, &mut |node| {
                if let Some(new) = library_links::reprefixed(&node.node_type_name, alias, new_alias)
                {
                    node.node_type_name = new;
                }
            });
        }
        self.push_command(RenameLibraryAliasCommand {
            old_alias: alias.to_string(),
            new_alias: new_alias.to_string(),
        });
        self.set_dirty(true);
        self.mark_full_refresh();
        Ok(())
    }

    /// The `# linked from …` line `query` prints for a network that belongs
    /// to a linked library (§6: read-only; the file to open to edit it).
    pub fn linked_network_header(&self, network_name: &str) -> Option<String> {
        let links = &self.node_type_registry.library_links;
        let mount = links.mount_containing(network_name)?;
        let via = match &mount.parent {
            Some(parent) => format!(", through `{}`", parent),
            None => String::new(),
        };
        Some(format!(
            "# linked from {} (library `{}`{}) — read-only; open the library file to edit it\n",
            mount.rel_path, mount.mount_path, via
        ))
    }

    /// The active network in the text format, as `query` shows it: the
    /// `# Network:` header, then — for a linked network — the `# linked from`
    /// line.
    pub fn query_active_network_text(&self) -> String {
        let Some(name) = self.active_node_network_name.as_deref() else {
            return "# No active node network\n".to_string();
        };
        let Some(network) = self.node_type_registry.node_networks.get(name) else {
            return format!("# Network '{}' not found\n", name);
        };
        let text =
            crate::text_format::serialize_network(network, &self.node_type_registry, Some(name));
        match self.linked_network_header(name) {
            Some(header) => match text.split_once('\n') {
                Some((first, rest)) => format!("{}\n{}{}", first, header, rest),
                None => format!("{}{}", header, text),
            },
            None => text,
        }
    }

    /// True when `name` (a network, record def or folder) belongs to a linked
    /// library (D3's prefix test).
    pub fn is_linked_name(&self, name: &str) -> bool {
        self.node_type_registry
            .library_links
            .mount_containing(name)
            .is_some()
    }

    /// True when the active network belongs to a linked library.
    pub fn active_network_is_linked(&self) -> bool {
        self.active_node_network_name
            .as_deref()
            .is_some_and(|n| self.is_linked_name(n))
    }

    /// The entity-layer read-only guard (§6): `Err` when `name` belongs to a
    /// linked library. Every `StructureDesigner` entry point that mutates the
    /// *content* of a network or a record def calls this (or
    /// [`ensure_active_editable`](Self::ensure_active_editable)) before it
    /// touches anything, so a refused edit leaves no partial mutation and no
    /// undo entry. View state (selection, camera, canvas viewport, display
    /// toggles) is not content and is not guarded (§5.4).
    pub fn ensure_editable(&self, name: &str) -> Result<(), String> {
        match self.node_type_registry.library_links.mount_containing(name) {
            Some(mount) => Err(format!(
                "'{}' is linked from '{}' and is read-only; open the library file to edit it",
                name, mount.rel_path
            )),
            None => Ok(()),
        }
    }

    /// [`ensure_editable`](Self::ensure_editable) for the active network
    /// (which is where every scoped edit lands). `Ok` when there is no active
    /// network — the caller's own "no active network" path handles that.
    pub fn ensure_active_editable(&self) -> Result<(), String> {
        match self.active_node_network_name.as_deref() {
            Some(name) => self.ensure_editable(name),
            None => Ok(()),
        }
    }

    /// True when a namespace operation on `prefix` would silently change a
    /// mount's alias (D3): the prefix is a mount, contains one, or lies inside
    /// one.
    pub fn namespace_touches_mount(&self, prefix: &str) -> bool {
        self.node_type_registry
            .library_links
            .iter()
            .any(|m| is_under(&m.mount_path, prefix) || is_under(prefix, &m.mount_path))
    }
}
