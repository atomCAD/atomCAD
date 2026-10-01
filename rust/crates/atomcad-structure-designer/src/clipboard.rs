//! The clipboard (`doc/design_multiple_documents.md` D9).
//!
//! There is one clipboard per session: it is app state, handed from document
//! to document at every tab switch (`StructureDesigner::hand_over_app_state`).
//! Along with the copied nodes it records where they came from — the source
//! document, the folder their relative file paths resolve against, and for
//! every user name they refer to, the file that defines it, the name it has
//! *inside that file* and its interface.
//!
//! A paste into the document the nodes were copied from uses them as they are.
//! A paste into another document maps every name to what that document calls
//! the same definition — through its own file or through one of its mounts —
//! and refuses, all or nothing, when a name is unreachable there or its
//! definition has a different interface. Relative file paths are re-spelled on
//! every paste, so they keep naming the same file.
//!
//! **What a node can refer to by name** is the library-linking list: a custom
//! network through `node_type_name`, a record def through
//! [`collect_record_refs_in_node`] (rewritten by
//! [`rewrite_record_names_in_node`]), and a data file through
//! `NodeData::file_paths`. A new kind of name reference in node data must be
//! added to both enumerations, or a cross-document paste silently keeps the
//! old name.

use crate::data_type::{DataType, walk_data_type_record_names_mut};
use crate::document_set::DocumentId;
use crate::file_dependencies::{canonical_or_lexical, path_key};
use crate::library_links::{LibraryMount, reprefixed};
use crate::library_refresh::{base_dir_of, rebase_data_path};
use crate::node_network::{NodeNetwork, walk_all_nodes, walk_all_nodes_mut};
use crate::node_type::PinOutputType;
use crate::node_type_registry::{
    NodeTypeRegistry, collect_record_refs_in_node, rewrite_record_names_in_node,
};
use crate::structure_designer::StructureDesigner;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};

/// Copied nodes and where they came from.
#[derive(Clone)]
pub struct Clipboard {
    /// The copied nodes, centred at the origin; wires only among them.
    pub nodes: NodeNetwork,
    pub origin: ClipboardOrigin,
}

/// Where the clipboard's nodes were copied. A snapshot taken at copy time;
/// only a rename or delete in the source document itself updates it
/// (`StructureDesigner::edit_own_clipboard`).
#[derive(Clone, Debug, Default)]
pub struct ClipboardOrigin {
    pub document: DocumentId,
    /// The folder the copied network's relative file paths resolve against
    /// (`library_refresh::base_dir_of`), canonical; `None` for an Untitled
    /// source.
    pub base_dir: Option<PathBuf>,
    /// Every user name the nodes refer to, as the source spells it, with the
    /// definition it resolved to.
    pub owners: BTreeMap<String, Owner>,
    /// User names the nodes refer to that do not resolve in the source: a
    /// frozen node's name under a mount, or a dangling local one. Nothing
    /// can be checked about them, so they paste only into the source.
    pub unresolved: BTreeSet<String>,
}

/// The definition a referenced name resolves to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Owner {
    /// The canonical path of the file defining the name; `None` when that is
    /// the source document itself and it is Untitled.
    pub file: Option<PathBuf>,
    /// The name inside that file: the mount path stripped
    /// (`demolib.common.bar` → `bar`).
    pub name: String,
    pub interface: Interface,
}

/// A definition's interface, as library linking D13 records it, with every
/// record name inside a type spelled as `<file>::<name in file>` — so the
/// interfaces of one definition seen from two documents, which spell its
/// record types differently, compare equal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Interface {
    Network {
        /// Per parameter, in pin order: `param_id`, name, type.
        params: Vec<(Option<u64>, String, String)>,
        /// Per output pin: name, type.
        outputs: Vec<(String, String)>,
    },
    Record {
        /// Per field, in authored order: `FieldId`, name, type.
        fields: Vec<(u64, String, String)>,
    },
}

/// Why a paste into another document was refused: one `(name, reason)` line
/// per offending name, as the source spells it. Nothing was pasted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteRefusal {
    pub lines: Vec<(String, String)>,
}

impl fmt::Display for PasteRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Nothing was pasted:")?;
        for (_, reason) in &self.lines {
            write!(f, "\n- {}", reason)?;
        }
        Ok(())
    }
}

/// What an edit of the clipboard did, for `StructureDesigner::edit_own_clipboard`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardEdit {
    Unchanged,
    /// Names were rewritten; the owners are recomputed.
    Changed,
    /// The clipboard refers to something that no longer exists.
    Clear,
}

/// The clipboard in `slot` if it was copied in `document`, else `None`. The
/// one guard every upkeep site goes through: a rename or delete in a
/// document must not touch names that belong to another one.
pub fn own_clipboard(slot: &mut Option<Clipboard>, document: DocumentId) -> Option<&mut Clipboard> {
    slot.as_mut().filter(|c| c.origin.document == document)
}

/// The clipboard as an undo or redo sees it (`UndoContext::clipboard`): the
/// document's own clipboard, or nothing when it was copied in another
/// document. A command that renames or deletes a network or record def
/// applies the same upkeep here that the forward operation applied, so the
/// clipboard follows an undone rename back. `changed` tells
/// `StructureDesigner` to recompute the owners afterwards.
pub struct ClipboardSlot<'a> {
    slot: Option<&'a mut Option<Clipboard>>,
    pub changed: bool,
}

impl<'a> ClipboardSlot<'a> {
    /// No clipboard to keep up (tests that drive commands directly).
    pub fn none() -> Self {
        ClipboardSlot {
            slot: None,
            changed: false,
        }
    }

    /// `slot` if the clipboard in it was copied in `document`.
    pub fn own(slot: &'a mut Option<Clipboard>, document: DocumentId) -> Self {
        let own = slot.as_ref().is_some_and(|c| c.origin.document == document);
        ClipboardSlot {
            slot: own.then_some(slot),
            changed: false,
        }
    }

    fn clipboard(&mut self) -> Option<&mut Clipboard> {
        self.slot.as_deref_mut().and_then(Option::as_mut)
    }

    /// Network or record renames, `(from, to)`.
    pub fn rename(&mut self, renames: &[(String, String)]) {
        if let Some(c) = self.clipboard()
            && c.rename(renames)
        {
            self.changed = true;
        }
    }

    /// Every name under `from` moves under `to` (a library alias rename).
    pub fn reprefix(&mut self, from: &str, to: &str) {
        if let Some(c) = self.clipboard()
            && c.reprefix(from, to)
        {
            self.changed = true;
        }
    }

    /// Clears the clipboard if it refers to a name `names` accepts (the redo
    /// of a delete).
    pub fn clear_if_refers_to(&mut self, names: &dyn Fn(&str) -> bool) {
        if self.clipboard().is_some_and(|c| c.refers_to_any(names))
            && let Some(slot) = self.slot.as_deref_mut()
        {
            *slot = None;
        }
    }
}

impl Clipboard {
    /// The clipboard for `nodes`, copied out of network `network_name` of
    /// `designer` (`copy_selection`).
    pub fn capture(designer: &StructureDesigner, network_name: &str, nodes: NodeNetwork) -> Self {
        let registry = &designer.node_type_registry;
        let base_dir = base_dir_of(registry, network_name).map(|d| canonical(registry, &d));
        let mut clipboard = Clipboard {
            nodes,
            origin: ClipboardOrigin {
                document: designer.document_id,
                base_dir,
                ..Default::default()
            },
        };
        clipboard.recapture_owners(designer);
        clipboard
    }

    /// Recomputes the owners from `designer`, the source document, after its
    /// names were rewritten in the clipboard.
    pub fn recapture_owners(&mut self, designer: &StructureDesigner) {
        let registry = &designer.node_type_registry;
        let own_file = document_file(designer);
        let mut owners = BTreeMap::new();
        let mut unresolved = BTreeSet::new();
        for name in referenced_names(registry, &self.nodes) {
            match owner_of(registry, own_file.as_deref(), &name) {
                Some(owner) => {
                    owners.insert(name, owner);
                }
                None => {
                    unresolved.insert(name);
                }
            }
        }
        self.origin.owners = owners;
        self.origin.unresolved = unresolved;
    }

    /// The nodes to paste into network `target_network` of `target` (§5.1):
    /// as copied when `target` is the source document, otherwise with every
    /// name mapped to `target`'s spelling — or the refusal when one cannot
    /// be. Relative file paths are rebased in both cases.
    pub fn for_target(
        &self,
        target: &StructureDesigner,
        target_network: &str,
    ) -> Result<NodeNetwork, PasteRefusal> {
        let registry = &target.node_type_registry;
        let mut nodes = self.nodes.clone();
        if target.document_id != self.origin.document {
            let names = self.translation(target)?;
            walk_all_nodes_mut(&mut nodes, &mut |node| {
                if let Some(to) = names.get(&node.node_type_name) {
                    node.node_type_name = to.clone();
                    node.custom_node_type = None;
                }
                rewrite_record_names_in_node(node, &mut |name: &mut String| {
                    if let Some(to) = names.get(name.as_str()) {
                        *name = to.clone();
                    }
                });
            });
            registry.initialize_custom_node_types_for_network(&mut nodes);
        }
        if let Some(from_dir) = &self.origin.base_dir {
            let to_dir = base_dir_of(registry, target_network).map(|d| canonical(registry, &d));
            let rebase = |stored: &str| rebase_data_path(from_dir, to_dir.as_deref(), stored);
            walk_all_nodes_mut(&mut nodes, &mut |node| {
                node.data.rebase_file_paths(&rebase);
            });
        }
        Ok(nodes)
    }

    /// Source spelling → `target` spelling for every owned name, or every
    /// reason it cannot be pasted there.
    fn translation(
        &self,
        target: &StructureDesigner,
    ) -> Result<HashMap<String, String>, PasteRefusal> {
        let registry = &target.node_type_registry;
        let target_file = document_file(target);
        let target_label = match &target_file {
            Some(f) => format!("`{}`", file_name(f)),
            None => "this Untitled design".to_string(),
        };
        let mut lines = Vec::new();
        for name in &self.origin.unresolved {
            lines.push((
                name.clone(),
                format!(
                    "`{}` does not resolve where it was copied, so it cannot be pasted into another document.",
                    name
                ),
            ));
        }
        let mut names = HashMap::new();
        for (name, owner) in &self.origin.owners {
            let Some(file) = &owner.file else {
                lines.push((
                    name.clone(),
                    format!(
                        "`{}` is defined in an Untitled design; save that design and link it here first.",
                        name
                    ),
                ));
                continue;
            };
            let key = path_key(file);
            let is_own_file = target_file.as_deref().map(path_key) == Some(key.clone());
            let mapped = if is_own_file {
                Some(owner.name.clone())
            } else {
                preferred_mount(registry, &key).map(|m| format!("{}.{}", m.mount_path, owner.name))
            };
            let Some(mapped) = mapped else {
                lines.push((
                    name.clone(),
                    format!(
                        "`{}` is defined in `{}`, which {} does not link.",
                        name,
                        file_name(file),
                        target_label
                    ),
                ));
                continue;
            };
            let fix = if is_own_file {
                format!(
                    "save `{}` and refresh it where you copied from",
                    file_name(file)
                )
            } else {
                format!("save `{}`, or refresh it here", file_name(file))
            };
            // In the target's own file the name must be local, not under a
            // mount that happens to spell it the same way.
            let resolved =
                if is_own_file && registry.library_links.mount_containing(&mapped).is_some() {
                    None
                } else {
                    owner_of(registry, target_file.as_deref(), &mapped)
                };
            match resolved {
                None => lines.push((
                    name.clone(),
                    format!(
                        "`{}` is not defined in this design as `{}` defines it — {}.",
                        mapped,
                        file_name(file),
                        fix
                    ),
                )),
                Some(found) if found.interface != owner.interface => {
                    let what = match owner.interface {
                        Interface::Network { .. } => "parameters",
                        Interface::Record { .. } => "fields",
                    };
                    lines.push((
                        name.clone(),
                        format!(
                            "`{}` has different {} in this design than where it was copied — {}.",
                            mapped, what, fix
                        ),
                    ));
                }
                Some(_) => {
                    names.insert(name.clone(), mapped);
                }
            }
        }
        if lines.is_empty() {
            Ok(names)
        } else {
            Err(PasteRefusal { lines })
        }
    }

    /// Network or record renames (`(old, new)` pairs) in the source.
    /// Returns whether anything changed.
    pub fn rename(&mut self, renames: &[(String, String)]) -> bool {
        if renames.is_empty() {
            return false;
        }
        let map: HashMap<&str, &str> = renames
            .iter()
            .map(|(old, new)| (old.as_str(), new.as_str()))
            .collect();
        self.rewrite_names(&mut |name| map.get(name).map(|n| n.to_string()))
    }

    /// Every name under `from` moves under `to` (a library alias rename).
    pub fn reprefix(&mut self, from: &str, to: &str) -> bool {
        self.rewrite_names(&mut |name| reprefixed(name, from, to))
    }

    fn rewrite_names(&mut self, rename: &mut dyn FnMut(&str) -> Option<String>) -> bool {
        let mut changed = false;
        walk_all_nodes_mut(&mut self.nodes, &mut |node| {
            if let Some(new) = rename(&node.node_type_name) {
                node.node_type_name = new;
                changed = true;
            }
            rewrite_record_names_in_node(node, &mut |name: &mut String| {
                if let Some(new) = rename(name) {
                    *name = new;
                    changed = true;
                }
            });
        });
        changed
    }

    /// True when a node — at any depth — refers to one of `names` as a
    /// network or a record def.
    pub fn refers_to_any(&self, names: &dyn Fn(&str) -> bool) -> bool {
        let mut found = false;
        walk_all_nodes(&self.nodes, &mut |node| {
            if found {
                return;
            }
            if names(&node.node_type_name) {
                found = true;
            }
            collect_record_refs_in_node(node, &mut |name, _| {
                if !name.is_empty() && names(name) {
                    found = true;
                }
            });
        });
        found
    }
}

/// The user names `nodes` refer to (bodies included): non-built-in node
/// types and non-built-in, non-empty record names.
fn referenced_names(registry: &NodeTypeRegistry, nodes: &NodeNetwork) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    walk_all_nodes(nodes, &mut |node| {
        if !registry
            .built_in_node_types
            .contains_key(&node.node_type_name)
        {
            names.insert(node.node_type_name.clone());
        }
        collect_record_refs_in_node(node, &mut |name, _| {
            if !name.is_empty() && !registry.built_in_record_type_defs.contains_key(name) {
                names.insert(name.to_string());
            }
        });
    });
    names
}

/// The definition `name` resolves to in `registry`, whose document's own
/// file is `own_file`; `None` when it does not resolve.
fn owner_of(registry: &NodeTypeRegistry, own_file: Option<&Path>, name: &str) -> Option<Owner> {
    let interface = interface_of(registry, own_file, name)?;
    let (file, name) = file_and_local_name(registry, own_file, name);
    Some(Owner {
        file,
        name,
        interface,
    })
}

/// The file defining `name` and the name inside it: the innermost mount
/// containing it, else the document's own file.
fn file_and_local_name(
    registry: &NodeTypeRegistry,
    own_file: Option<&Path>,
    name: &str,
) -> (Option<PathBuf>, String) {
    match registry.library_links.mount_containing(name) {
        Some(mount) => (
            Some(canonical(registry, &mount.abs_path)),
            name[mount.mount_path.len() + 1..].to_string(),
        ),
        None => (own_file.map(Path::to_path_buf), name.to_string()),
    }
}

fn interface_of(
    registry: &NodeTypeRegistry,
    own_file: Option<&Path>,
    name: &str,
) -> Option<Interface> {
    let type_key = |t: &DataType| type_key(registry, own_file, t);
    if let Some(network) = registry.node_networks.get(name) {
        return Some(Interface::Network {
            params: crate::network_validator::live_parameters(network)
                .iter()
                .map(|p| (p.id, p.name.clone(), type_key(&p.data_type)))
                .collect(),
            outputs: network
                .node_type
                .output_pins
                .iter()
                .map(|pin| {
                    let t = match &pin.data_type {
                        PinOutputType::Fixed(t) => type_key(t),
                        other => other.to_string(),
                    };
                    (pin.name.clone(), t)
                })
                .collect(),
        });
    }
    let def = registry.record_type_defs.get(name)?;
    Some(Interface::Record {
        fields: def
            .fields
            .iter()
            .map(|f| (f.id.0, f.name.clone(), type_key(&f.data_type)))
            .collect(),
    })
}

/// `t` with every user record name spelled `<file>::<name in file>`.
fn type_key(registry: &NodeTypeRegistry, own_file: Option<&Path>, t: &DataType) -> String {
    let mut t = t.clone();
    walk_data_type_record_names_mut(&mut t, &mut |name: &mut String| {
        if registry.record_type_defs.contains_key(name.as_str()) {
            let (file, local) = file_and_local_name(registry, own_file, name);
            let file = file.map_or_else(|| "<untitled>".to_string(), |f| path_key(&f));
            *name = format!("{}::{}", file, local);
        }
    });
    t.to_string()
}

/// The mount of `registry` that loads the file with key `file_key`: a direct
/// mount first, then the shortest mount path.
fn preferred_mount<'a>(registry: &'a NodeTypeRegistry, file_key: &str) -> Option<&'a LibraryMount> {
    registry
        .library_links
        .iter()
        .filter(|m| path_key(&canonical(registry, &m.abs_path)) == file_key)
        .min_by_key(|m| (!m.is_direct(), m.mount_path.len(), m.mount_path.clone()))
}

/// The canonical path of `designer`'s own file, if it has one.
fn document_file(designer: &StructureDesigner) -> Option<PathBuf> {
    let path = designer.file_path.as_ref()?;
    Some(canonical(&designer.node_type_registry, Path::new(path)))
}

fn canonical(registry: &NodeTypeRegistry, path: &Path) -> PathBuf {
    canonical_or_lexical(registry.library_links.fs().as_ref(), path)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}
