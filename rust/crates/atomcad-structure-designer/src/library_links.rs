//! Library linking: a `.cnnd` file that uses another `.cnnd` file by reference.
//!
//! Design doc: `doc/design_library_linking.md`. This module is Phase 1 of it —
//! the mount model, the file-format hooks, and the frozen-node predicate.
//!
//! # The model in one paragraph
//!
//! A linked library is loaded into a **temporary** registry by the same load
//! function the host uses (so its own links recurse), every name in it is
//! **prefixed** with the alias the importing file chose, and the result is
//! moved into the host's one registry (D1). "Is this linked?" is then a pure
//! longest-prefix test on the name ([`LibraryLinks::mount_containing`], D3) —
//! there is no per-entity origin field. Save writes only what is *not* under a
//! mount, plus the import list (D5).
//!
//! # Frozen nodes (§8)
//!
//! A host node that refers to a name under a mount that does not resolve — the
//! library is missing, unparseable, part of a cycle, or simply no longer
//! defines the name — is **frozen**: no repair pass may grow, truncate or
//! realign its arguments, and no pass may disconnect a wire into or out of it,
//! nor a wire whose type check involves such a name. The one predicate is
//! [`unresolved_mount_ref`]; every repair pass that drops or realigns a wire
//! must consult it (through [`is_frozen`] / [`data_type_mentions_unresolved_mount`]).
//!
//! # Disk access
//!
//! Everything reads and writes through [`LinkFs`], so a library is read **once**
//! per mount (its stamp and hash describe exactly the bytes parsed) and every
//! `.cnnd` write goes to a temp file that is renamed over the target
//! ([`write_atomic`]).

use crate::data_type::{DataType, walk_data_type_record_names};
use crate::node_network::{Node, NodeNetwork, walk_all_nodes, walk_all_nodes_mut};
use crate::node_type_registry::{
    NodeTypeRegistry, RecordRefSite, RecordTypeDef, collect_record_refs_in_node,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

// ---------------------------------------------------------------------------
// Names and paths
// ---------------------------------------------------------------------------

/// True iff `name` is `prefix` itself or lies under it (`prefix.` + more).
/// The one prefix test of D3; never use a bare `starts_with(prefix)`, which
/// would take `demolibx` for part of `demolib`.
pub fn is_under(name: &str, prefix: &str) -> bool {
    name == prefix
        || (name.len() > prefix.len()
            && name.starts_with(prefix)
            && name.as_bytes()[prefix.len()] == b'.')
}

/// Validates a (possibly dotted) alias: every segment must be an identifier
/// (`[A-Za-z_][A-Za-z0-9_]*`). Stricter than `is_valid_user_name` on purpose —
/// the alias becomes the leading segments of every linked name, which the text
/// format must be able to spell as a node type.
pub fn validate_alias(alias: &str) -> Result<(), String> {
    if alias.is_empty() {
        return Err("alias must not be empty".to_string());
    }
    for segment in alias.split('.') {
        let mut chars = segment.chars();
        let valid = match chars.next() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
            }
            _ => false,
        };
        if !valid {
            return Err(format!(
                "alias '{}' has an invalid segment '{}' (each segment must be an identifier)",
                alias, segment
            ));
        }
    }
    Ok(())
}

/// Lexically normalizes a library path as stored in an `imports` entry:
/// forward slashes, no `.` segments, `a/..` collapsed (leading `..` kept).
/// Absolute paths are refused (D6).
pub fn normalize_rel_path(path: &str) -> Result<String, String> {
    let unified = path.replace('\\', "/");
    if unified.starts_with('/') || Path::new(path).is_absolute() || has_drive_prefix(&unified) {
        return Err(format!(
            "library path '{}' is absolute; libraries are linked by a relative path",
            path
        ));
    }
    let mut out: Vec<&str> = Vec::new();
    for segment in unified.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if matches!(out.last(), Some(s) if *s != "..") {
                    out.pop();
                } else {
                    out.push("..");
                }
            }
            s => out.push(s),
        }
    }
    if out.is_empty() || out.last() == Some(&"..") {
        return Err(format!("library path '{}' does not name a file", path));
    }
    Ok(out.join("/"))
}

fn has_drive_prefix(p: &str) -> bool {
    let b = p.as_bytes();
    b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic()
}

/// Joins a stored relative path onto the importing file's directory, lexically.
/// Errors when the path climbs above the filesystem root.
pub fn lexical_join(base_dir: &Path, rel: &str) -> Result<PathBuf, String> {
    let mut parts: Vec<Component> = base_dir.components().collect();
    let unified = rel.replace('\\', "/");
    for segment in unified.split('/') {
        match segment {
            "" | "." => {}
            ".." => match parts.last() {
                Some(Component::Normal(_)) => {
                    parts.pop();
                }
                Some(Component::ParentDir) | None => parts.push(Component::ParentDir),
                Some(_) => {
                    return Err(format!(
                        "library path '{}' climbs above the filesystem root",
                        rel
                    ));
                }
            },
            s => parts.push(Component::Normal(std::ffi::OsStr::new(s))),
        }
    }
    let mut out = PathBuf::new();
    for part in parts {
        out.push(part.as_os_str());
    }
    Ok(out)
}

/// The relative path (forward slashes, `..` allowed) from directory `from_dir`
/// to `to`, or `None` when none exists (another drive on Windows). Both paths
/// should be canonical.
pub fn relative_path(from_dir: &Path, to: &Path) -> Option<String> {
    let from: Vec<Component> = from_dir.components().collect();
    let to: Vec<Component> = to.components().collect();
    // Roots (prefix + root dir) must agree, or there is no relative path.
    let root_len = |c: &[Component]| {
        c.iter()
            .take_while(|c| matches!(c, Component::Prefix(_) | Component::RootDir))
            .count()
    };
    let (fr, tr) = (root_len(&from), root_len(&to));
    if fr != tr || from[..fr] != to[..tr] {
        return None;
    }
    let common = from
        .iter()
        .zip(to.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let mut segments: Vec<String> = Vec::new();
    for _ in common..from.len() {
        segments.push("..".to_string());
    }
    for c in &to[common..] {
        segments.push(c.as_os_str().to_string_lossy().to_string());
    }
    if segments.is_empty() {
        return None;
    }
    Some(segments.join("/"))
}

// ---------------------------------------------------------------------------
// Disk access
// ---------------------------------------------------------------------------

/// What `stat` reports about a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileMeta {
    pub mtime: Option<SystemTime>,
    pub size: u64,
}

/// All disk access of library linking goes through this trait, so tests can
/// substitute a fault-injecting implementation (§11.1). [`RealFs`] is the only
/// production implementation.
pub trait LinkFs: Send + Sync {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn stat(&self, path: &Path) -> io::Result<FileMeta>;
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    /// What is at `path` itself, without following a final symlink. The Save
    /// As copy refuses to write through a link or onto a folder (D11).
    fn entry_kind(&self, path: &Path) -> io::Result<EntryKind> {
        let file_type = std::fs::symlink_metadata(path)?.file_type();
        Ok(if file_type.is_symlink() {
            EntryKind::Symlink
        } else if file_type.is_dir() {
            EntryKind::Dir
        } else {
            EntryKind::File
        })
    }
}

/// What [`LinkFs::entry_kind`] reports. A Windows junction counts as a
/// symlink.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
}

/// The real filesystem.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealFs;

impl LinkFs for RealFs {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }
    fn stat(&self, path: &Path) -> io::Result<FileMeta> {
        let meta = std::fs::metadata(path)?;
        Ok(FileMeta {
            mtime: meta.modified().ok(),
            size: meta.len(),
        })
    }
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        std::fs::write(path, bytes)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }
}

/// Writes `bytes` to a temp file beside `path` and renames it over `path`, so a
/// failed write never leaves a truncated file (§5.1). On a failed rename the
/// temp file is removed and the target is untouched.
pub fn write_atomic(fs: &dyn LinkFs, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let tmp_name = format!(".{}.tmp-{}", file_name, std::process::id());
    let tmp = match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(tmp_name),
        _ => PathBuf::from(tmp_name),
    };
    if let Err(e) = fs.write(&tmp, bytes) {
        let _ = fs.remove_file(&tmp);
        return Err(e);
    }
    if let Err(e) = fs.rename(&tmp, path) {
        let _ = fs.remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Mount records
// ---------------------------------------------------------------------------

/// State of one mount. Only `Loaded` (and, from Phase 3 on, the two
/// "older than disk" markers) have content under the mount path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MountStatus {
    Loaded,
    /// Memory is deliberately older than the disk (after an undone refresh; P3).
    OlderThanDisk,
    /// A disk change was detected but held because redo history exists (P3).
    ChangedOnDisk,
    /// The file does not exist.
    Missing,
    /// The file could not be read or parsed, or the link is otherwise invalid.
    Error(String),
    /// The file is already being loaded further up the link chain.
    Cycle,
}

impl MountStatus {
    /// True when the mount carries content (the library was read and parsed).
    pub fn has_content(&self) -> bool {
        matches!(
            self,
            MountStatus::Loaded | MountStatus::OlderThanDisk | MountStatus::ChangedOnDisk
        )
    }

    pub fn message(&self) -> String {
        match self {
            MountStatus::Loaded => "loaded".to_string(),
            MountStatus::OlderThanDisk => "older than the file on disk".to_string(),
            MountStatus::ChangedOnDisk => "changed on disk".to_string(),
            MountStatus::Missing => "file not found".to_string(),
            MountStatus::Error(e) => e.clone(),
            MountStatus::Cycle => {
                "the library links back to a file that links it (cycle)".to_string()
            }
        }
    }
}

/// `(mtime, size, blake3)` of one version of a watched file (D7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileStamp {
    pub mtime: Option<SystemTime>,
    pub size: u64,
    /// Hex BLAKE3 of the bytes.
    pub blake3: String,
}

impl FileStamp {
    pub fn of_bytes(bytes: &[u8], mtime: Option<SystemTime>) -> Self {
        FileStamp {
            mtime,
            size: bytes.len() as u64,
            blake3: blake3::hash(bytes).to_hex().to_string(),
        }
    }

    /// The form written to an `imports` entry's `hash` field.
    pub fn hash_string(&self) -> String {
        format!("b3:{}", self.blake3)
    }
}

/// One parameter of a recorded network interface.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsedParam {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: String,
}

/// One output pin of a recorded network interface.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsedOutput {
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: String,
}

/// One field of a recorded record interface.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsedField {
    pub id: u64,
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsedNetworkInterface {
    #[serde(default)]
    pub params: Vec<UsedParam>,
    #[serde(default)]
    pub outputs: Vec<UsedOutput>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsedRecordInterface {
    #[serde(default)]
    pub fields: Vec<UsedField>,
}

/// The interfaces an importing file is wired against (D13), keyed by name
/// **relative to the import's alias**. Serialized as an import's `uses` table.
/// Types are written as the importing file spells them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsedInterfaces {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub networks: BTreeMap<String, UsedNetworkInterface>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub records: BTreeMap<String, UsedRecordInterface>,
}

impl UsedInterfaces {
    pub fn is_empty(&self) -> bool {
        self.networks.is_empty() && self.records.is_empty()
    }
}

/// One linked library as mounted in a registry.
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryMount {
    /// `"libs.demolib"` for a direct mount, `"libs.demolib.common"` for one of
    /// its own links.
    pub mount_path: String,
    /// As written by the importing file (may be dotted).
    pub alias: String,
    /// As written by the importing file (never rewritten, D6).
    pub rel_path: String,
    /// Canonical when the file exists; the lexical join otherwise.
    pub abs_path: PathBuf,
    /// `mount_path` of the importing mount; `None` = a direct link of the file
    /// this registry belongs to.
    pub parent: Option<String>,
    pub status: MountStatus,
    /// Stamp of the content in memory.
    pub loaded: Option<FileStamp>,
    /// Stamp last observed on disk (D7).
    pub last_seen: Option<FileStamp>,
    /// `hash` as read from the importing file (D13: a hint only).
    pub stored_hash: Option<String>,
    /// `uses` as read from the importing file (D13); after a refresh, the
    /// interfaces the importing file was wired against just before it
    /// (§7.1 step 1).
    pub stored_uses: UsedInterfaces,
    /// Per network of this mount (full name), the `param_id`s the load-time
    /// duplicate-id heal found shared by several parameters. Reconciliation
    /// matches those by name only (`network_validator::parameter_mapping`).
    pub name_only_param_ids: BTreeMap<String, BTreeSet<u64>>,
}

impl LibraryMount {
    pub fn is_direct(&self) -> bool {
        self.parent.is_none()
    }

    /// The `hash` an importing file writes back for this mount: the loaded
    /// content's when there is content in memory (also when the file has
    /// since gone missing — the content is still what the host is wired
    /// against), else the one read from the file.
    pub fn hash_for_save(&self) -> Option<String> {
        match &self.loaded {
            Some(stamp) => Some(stamp.hash_string()),
            None => self.stored_hash.clone(),
        }
    }
}

/// The mounts of one registry, keyed by mount path, and the watch state of
/// the data files its networks read (D7).
#[derive(Clone)]
pub struct LibraryLinks {
    mounts: BTreeMap<String, LibraryMount>,
    fs: Arc<dyn LinkFs>,
    /// Data files read at load time by the host's and the libraries' nodes,
    /// with their `loaded` / `last_seen` stamps (`library_refresh`).
    pub data_files:
        BTreeMap<crate::library_refresh::DataFileKey, crate::library_refresh::DataFileWatch>,
    /// What the last load reconciled (D13), drained by
    /// `StructureDesigner::load_node_networks`.
    pub load_report: crate::library_refresh::RefreshReport,
    /// The wires of the reconciled local networks right after the last load
    /// reconciled them, so that whatever the later repair and validation
    /// passes remove is reported too (`library_refresh::report_wires_removed_by_repair`).
    pub load_images: Vec<(String, crate::library_refresh::WireImages)>,
}

impl Default for LibraryLinks {
    fn default() -> Self {
        Self {
            mounts: BTreeMap::new(),
            fs: Arc::new(RealFs),
            data_files: BTreeMap::new(),
            load_report: Default::default(),
            load_images: Vec::new(),
        }
    }
}

impl std::fmt::Debug for LibraryLinks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibraryLinks")
            .field("mounts", &self.mounts)
            .finish()
    }
}

impl LibraryLinks {
    pub fn is_empty(&self) -> bool {
        self.mounts.is_empty()
    }

    pub fn fs(&self) -> Arc<dyn LinkFs> {
        self.fs.clone()
    }

    /// Replaces the filesystem used for every later mount and save (tests).
    pub fn set_fs(&mut self, fs: Arc<dyn LinkFs>) {
        self.fs = fs;
    }

    pub fn get(&self, mount_path: &str) -> Option<&LibraryMount> {
        self.mounts.get(mount_path)
    }

    pub fn get_mut(&mut self, mount_path: &str) -> Option<&mut LibraryMount> {
        self.mounts.get_mut(mount_path)
    }

    pub fn iter(&self) -> impl Iterator<Item = &LibraryMount> {
        self.mounts.values()
    }

    /// Direct mounts, sorted by alias.
    pub fn direct_mounts(&self) -> impl Iterator<Item = &LibraryMount> {
        self.mounts.values().filter(|m| m.is_direct())
    }

    pub fn insert(&mut self, mount: LibraryMount) {
        self.mounts.insert(mount.mount_path.clone(), mount);
    }

    pub fn clear(&mut self) {
        self.mounts.clear();
        self.data_files.clear();
        self.load_report = Default::default();
        self.load_images.clear();
    }

    /// Removes and returns every mount record at or under `mount_path`.
    pub fn remove_subtree(&mut self, mount_path: &str) -> Vec<LibraryMount> {
        let keys: Vec<String> = self
            .mounts
            .keys()
            .filter(|k| is_under(k, mount_path))
            .cloned()
            .collect();
        keys.into_iter()
            .filter_map(|k| self.mounts.remove(&k))
            .collect()
    }

    /// The mount owning `name` — the longest mount path `p` with
    /// `is_under(name, p)` — or `None` for local content (D3).
    pub fn mount_containing(&self, name: &str) -> Option<&LibraryMount> {
        if self.mounts.is_empty() {
            return None;
        }
        self.mounts
            .values()
            .filter(|m| is_under(name, &m.mount_path))
            .max_by_key(|m| m.mount_path.len())
    }

    /// The direct mount whose subtree contains `name` (the import a host
    /// reference is recorded under, D13).
    pub fn direct_mount_containing(&self, name: &str) -> Option<&LibraryMount> {
        self.mounts
            .values()
            .find(|m| m.is_direct() && is_under(name, &m.mount_path))
    }
}

// ---------------------------------------------------------------------------
// The frozen-node predicate (§8)
// ---------------------------------------------------------------------------

fn name_under_mount_unresolved_network(registry: &NodeTypeRegistry, name: &str) -> bool {
    registry.library_links.mount_containing(name).is_some()
        && !registry.node_networks.contains_key(name)
        && !registry.built_in_node_types.contains_key(name)
}

fn name_under_mount_unresolved_record(registry: &NodeTypeRegistry, name: &str) -> bool {
    !name.is_empty()
        && registry.library_links.mount_containing(name).is_some()
        && registry.lookup_record_type_def(name).is_none()
}

/// The first name under a mount that `node` refers to and that does not
/// resolve, if any: its `node_type_name`, a record schema / target, or a
/// `Named` record inside any `DataType` stored in its data. Whatever the
/// mount's status — a `Loaded` mount that dropped the name counts. Names
/// outside every mount are not reported (they keep today's behaviour).
pub fn unresolved_mount_ref(node: &Node, registry: &NodeTypeRegistry) -> Option<String> {
    if registry.library_links.is_empty() {
        return None;
    }
    if name_under_mount_unresolved_network(registry, &node.node_type_name) {
        return Some(node.node_type_name.clone());
    }
    let mut found: Option<String> = None;
    collect_record_refs_in_node(node, &mut |name, _site| {
        if found.is_none() && name_under_mount_unresolved_record(registry, name) {
            found = Some(name.to_string());
        }
    });
    found
}

/// Shorthand for `unresolved_mount_ref(..).is_some()`.
pub fn is_frozen(node: &Node, registry: &NodeTypeRegistry) -> bool {
    unresolved_mount_ref(node, registry).is_some()
}

/// True when `t` mentions a `Named` record under a mount that does not
/// resolve. A repair pass must not disconnect a wire whose compatibility check
/// involves such a type (§8, rule 2).
pub fn data_type_mentions_unresolved_mount(t: &DataType, registry: &NodeTypeRegistry) -> bool {
    if registry.library_links.is_empty() {
        return false;
    }
    let mut found = false;
    walk_data_type_record_names(t, &mut |name| {
        if !found && name_under_mount_unresolved_record(registry, name) {
            found = true;
        }
    });
    found
}

/// The nodes of one scope whose arguments no repair pass may grow, truncate
/// or realign: the frozen nodes, plus every node fed by a frozen node's
/// function pin (`-1`). The second kind is not frozen itself, but its pin
/// layout is *derived* from the frozen source's signature (`apply`, a HOF `f`
/// pin), which cannot be derived while the source does not resolve — so a
/// count repair would cut its `arg0…` wires against the bare `[f]` layout.
pub fn protected_node_ids(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) -> std::collections::HashSet<u64> {
    let mut out = std::collections::HashSet::new();
    if registry.library_links.is_empty() {
        return out;
    }
    for (&id, node) in &network.nodes {
        if is_frozen(node, registry) {
            out.insert(id);
        }
    }
    let frozen = out.clone();
    for (&id, node) in &network.nodes {
        let fed_by_frozen_function = node.arguments.iter().any(|arg| {
            arg.incoming_wires.iter().any(|w| {
                w.source_scope_depth == 0
                    && frozen.contains(&w.source_node_id)
                    && matches!(
                        w.source_pin,
                        crate::node_network::SourcePin::NodeOutput { pin_index: -1 }
                    )
            })
        });
        if fed_by_frozen_function {
            out.insert(id);
        }
    }
    out
}

/// [`protected_node_ids`] for a single node of `network`: frozen, or fed by a
/// frozen node's function pin.
pub fn is_protected(node: &Node, network: &NodeNetwork, registry: &NodeTypeRegistry) -> bool {
    if registry.library_links.is_empty() {
        return false;
    }
    if is_frozen(node, registry) {
        return true;
    }
    node.arguments.iter().any(|arg| {
        arg.incoming_wires.iter().any(|w| {
            w.source_scope_depth == 0
                && matches!(
                    w.source_pin,
                    crate::node_network::SourcePin::NodeOutput { pin_index: -1 }
                )
                && network
                    .nodes
                    .get(&w.source_node_id)
                    .is_some_and(|source| is_frozen(source, registry))
        })
    })
}

/// The validation message for a frozen node.
pub fn frozen_node_message(node: &Node, name: &str) -> String {
    if node.node_type_name == name {
        format!(
            "Unknown node type `{}` (linked library not available)",
            name
        )
    } else {
        format!(
            "Unknown record type `{}` (linked library not available)",
            name
        )
    }
}

/// The mount path of the mount owning `network_name` — `None` for a local
/// network of the file the registry belongs to. This identifies **which
/// file** a node lives in, and so whose `uses` tables describe its wiring.
pub fn owner_mount(registry: &NodeTypeRegistry, network_name: &str) -> Option<String> {
    registry
        .library_links
        .mount_containing(network_name)
        .map(|m| m.mount_path.clone())
}

/// True when `name` is at or under a mount path, or is a strict prefix of one
/// (a folder that holds a mount). A new local entity or folder may be neither
/// (D3): the first would put local content into a library's folder, the second
/// would turn the folder holding a mount into an entity.
pub fn name_conflicts_with_mount(registry: &NodeTypeRegistry, name: &str) -> bool {
    registry
        .library_links
        .iter()
        .any(|m| is_under(name, &m.mount_path) || is_under(&m.mount_path, name))
}

/// The positional pin spelling of the text format (§13 item 6): `@<index>`.
/// `@` followed by digits is not a valid identifier, so it can never be a pin
/// name. Returns the index when `name` is such a spelling.
pub fn positional_pin_index(name: &str) -> Option<usize> {
    let digits = name.strip_prefix('@')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// The positional pin spelling of index `i`.
pub fn positional_pin_name(i: usize) -> String {
    format!("@{}", i)
}

// ---------------------------------------------------------------------------
// Recorded layouts of frozen nodes (§13 item 6)
// ---------------------------------------------------------------------------

fn parse_recorded_type(s: &str) -> DataType {
    DataType::from_string(s).unwrap_or(DataType::None)
}

/// The recorded interface a frozen name refers to, from the `uses` table of
/// the file that owns the node: the mount whose importing file is `owner`
/// (`None` = the host) and under which `name` lies. The key is `name`
/// relative to that mount.
pub(crate) fn recorded_entry<'r, T>(
    registry: &'r NodeTypeRegistry,
    name: &str,
    owner: Option<&str>,
    pick: impl Fn(&'r UsedInterfaces, &str) -> Option<&'r T>,
) -> Option<&'r T> {
    registry
        .library_links
        .iter()
        .filter(|m| m.parent.as_deref() == owner)
        .filter(|m| name.len() > m.mount_path.len() && is_under(name, &m.mount_path))
        .find_map(|m| pick(&m.stored_uses, &name[m.mount_path.len() + 1..]))
}

/// The pin layout a **frozen** node gets from the recorded interface of the
/// name it refers to (§13 item 6): an instance gets the entry's parameters
/// (with their ids) and outputs; a `record_construct` / `record_destructure`
/// / `product` gets the entry's fields, stamped exactly as
/// `build_node_type_for_schema_with_defs` would stamp them from a live def.
/// `None` when the node is not frozen, refers to the name only through a type,
/// or no `uses` entry records the name.
///
/// `owner` is the mount owning the network the node lives in (see
/// [`owner_mount`]).
pub fn recorded_layout(
    node: &Node,
    registry: &NodeTypeRegistry,
    owner: Option<&str>,
) -> Option<crate::node_type::NodeType> {
    use crate::node_type::{NodeTypeCategory, OutputPinDefinition, Parameter};
    let name = unresolved_mount_ref(node, registry)?;
    if node.node_type_name == name {
        let entry = recorded_entry(registry, &name, owner, |u, k| u.networks.get(k))?;
        return Some(crate::node_type::NodeType {
            name: name.clone(),
            description: String::new(),
            summary: None,
            category: NodeTypeCategory::Custom,
            parameters: entry
                .params
                .iter()
                .map(|p| Parameter {
                    id: p.id,
                    name: p.name.clone(),
                    data_type: parse_recorded_type(&p.data_type),
                })
                .collect(),
            output_pins: entry
                .outputs
                .iter()
                .map(|o| OutputPinDefinition::fixed(&o.name, parse_recorded_type(&o.data_type)))
                .collect(),
            node_data_creator: || Box::new(crate::node_data::CustomNodeData::default()),
            node_data_saver: crate::node_type::generic_node_data_saver::<
                crate::node_data::CustomNodeData,
            >,
            node_data_loader: crate::node_type::generic_node_data_loader::<
                crate::node_data::CustomNodeData,
            >,
            zone_input_pins: vec![],
            zone_output_pins: vec![],
            public: true,
        });
    }
    let schema = record_node_schema(node)?;
    if schema != name {
        return None;
    }
    let entry = recorded_entry(registry, &name, owner, |u, k| u.records.get(k))?;
    let def = RecordTypeDef {
        name: name.clone(),
        fields: entry
            .fields
            .iter()
            .map(|f| crate::node_type_registry::RecordField {
                id: crate::node_type_registry::FieldId(f.id),
                name: f.name.clone(),
                data_type: parse_recorded_type(&f.data_type),
                hint: None,
            })
            .collect(),
        next_field_id: entry.fields.iter().map(|f| f.id + 1).max().unwrap_or(0),
    };
    let defs: std::collections::HashMap<String, RecordTypeDef> =
        std::iter::once((name.clone(), def)).collect();
    let base = registry.built_in_node_types.get(&node.node_type_name)?;
    let built_in = &registry.built_in_record_type_defs;
    Some(match node.node_type_name.as_str() {
        "record_construct" => crate::nodes::record_construct::build_node_type_for_schema_with_defs(
            base, &name, &defs, built_in,
        ),
        "record_destructure" => {
            crate::nodes::record_destructure::build_node_type_for_schema_with_defs(
                base, &name, &defs, built_in,
            )
        }
        _ => crate::nodes::product::build_node_type_for_target_with_defs(
            base, &name, &defs, built_in,
        ),
    })
}

/// The record def a `record_construct` / `record_destructure` / `product`
/// node is built on.
pub(crate) fn record_node_schema(node: &Node) -> Option<String> {
    let data = node.data.as_any_ref();
    if let Some(d) = data.downcast_ref::<crate::nodes::record_construct::RecordConstructData>() {
        return Some(d.schema.clone());
    }
    if let Some(d) = data.downcast_ref::<crate::nodes::record_destructure::RecordDestructureData>()
    {
        return Some(d.schema.clone());
    }
    data.downcast_ref::<crate::nodes::product::ProductData>()
        .map(|d| d.target.clone())
}

/// Installs the recorded layout (§13 item 6) on `node` if it is frozen and
/// one is recorded. Never touches `arguments` (`refresh_args = false`).
/// Returns whether a layout was installed.
pub fn install_recorded_layout(
    node: &mut Node,
    registry: &NodeTypeRegistry,
    owner: Option<&str>,
) -> bool {
    match recorded_layout(node, registry, owner) {
        Some(layout) => {
            node.set_custom_node_type(Some(layout), false);
            true
        }
        None => false,
    }
}

/// [`install_recorded_layout`] over every node of `network`, bodies included.
pub fn install_recorded_layouts(
    network: &mut NodeNetwork,
    registry: &NodeTypeRegistry,
    owner: Option<&str>,
) {
    if registry.library_links.is_empty() {
        return;
    }
    walk_all_nodes_mut(network, &mut |node| {
        install_recorded_layout(node, registry, owner);
    });
}

// ---------------------------------------------------------------------------
// Transitive references (D4)
// ---------------------------------------------------------------------------

/// A name `node` refers to (as an instance or a record schema) that lies under
/// a **transitive** mount — one the owning file does not link directly. Such a
/// reference works, but breaks when the direct library stops linking it, so
/// validation warns about it (D4, Go's rule). Only meaningful for nodes of
/// local networks.
pub fn transitive_mount_ref(node: &Node, registry: &NodeTypeRegistry) -> Option<String> {
    if registry.library_links.is_empty() {
        return None;
    }
    let transitive = |name: &str| {
        registry
            .library_links
            .mount_containing(name)
            .is_some_and(|m| !m.is_direct())
    };
    if transitive(&node.node_type_name) {
        return Some(node.node_type_name.clone());
    }
    record_node_schema(node).filter(|s| transitive(s))
}

// ---------------------------------------------------------------------------
// Evaluation base directory (D8)
// ---------------------------------------------------------------------------

/// The name of the network the code on top of `network_stack` belongs to:
/// the innermost frame that is a *network*, or the innermost zone-body frame
/// that records the network its closure was defined in (a lazy walker runs a
/// body on a body-only stack). A zone-body frame without a record belongs to
/// the network below it.
pub fn stack_home(
    network_stack: &[crate::evaluator::network_evaluator::NetworkStackElement<'_>],
) -> Option<Arc<str>> {
    network_stack.iter().rev().find_map(|f| {
        if f.is_zone_body {
            f.home.clone()
        } else {
            Some(Arc::from(f.node_network.node_type.name.as_str()))
        }
    })
}

/// The directory a relative file path read or written by a node resolves
/// against **at evaluation time** (D8): the directory of the file that owns
/// the network the node lives in — the library's folder for a node in a linked
/// network, the design's folder for a local one. That network is
/// [`stack_home`]'s (a zone body belongs to the network it was defined in).
/// This is the only way to resolve a relative path at eval time.
pub fn base_dir_for_eval(
    network_stack: &[crate::evaluator::network_evaluator::NetworkStackElement<'_>],
    registry: &NodeTypeRegistry,
) -> Option<String> {
    let host_dir = || {
        registry
            .design_file_name
            .as_ref()
            .and_then(|p| atomcad_util::path_utils::get_parent_directory(p))
    };
    let Some(home) = stack_home(network_stack) else {
        return host_dir();
    };
    match registry.library_links.mount_containing(&home) {
        Some(mount) => mount
            .abs_path
            .parent()
            .map(|p| p.to_string_lossy().to_string()),
        None => host_dir(),
    }
}

// ---------------------------------------------------------------------------
// Recorded interfaces (D13)
// ---------------------------------------------------------------------------

/// The interface of `network` as recorded in a `uses` table. Parameters come
/// from the parameter nodes (`network_validator::live_parameters`), never from
/// `node_type.parameters`, which a network restored by an undo snapshot carries
/// without ids until it is next validated (§13 item 7).
pub fn network_interface(network: &NodeNetwork) -> UsedNetworkInterface {
    UsedNetworkInterface {
        params: crate::network_validator::live_parameters(network)
            .iter()
            .map(|p| UsedParam {
                id: p.id,
                name: p.name.clone(),
                data_type: p.data_type.to_string(),
            })
            .collect(),
        outputs: network
            .node_type
            .output_pins
            .iter()
            .map(|pin| UsedOutput {
                name: pin.name.clone(),
                data_type: pin.data_type.to_string(),
            })
            .collect(),
    }
}

/// The interface of `def` as recorded in a `uses` table.
pub fn record_interface(def: &RecordTypeDef) -> UsedRecordInterface {
    UsedRecordInterface {
        fields: def
            .fields
            .iter()
            .map(|f| UsedField {
                id: f.id.0,
                name: f.name.clone(),
                data_type: f.data_type.to_string(),
            })
            .collect(),
    }
}

/// Names under mount `direct_alias` that local nodes refer to as custom
/// network instances or record schemas (recursing into HOF bodies), split by
/// kind. Keys are the full names.
fn referenced_names(
    registry: &NodeTypeRegistry,
    direct_alias: &str,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut networks = BTreeSet::new();
    let mut records = BTreeSet::new();
    for (name, network) in &registry.node_networks {
        if registry.library_links.mount_containing(name).is_some() {
            continue;
        }
        walk_all_nodes(network, &mut |node| {
            if is_under(&node.node_type_name, direct_alias)
                && node.node_type_name != direct_alias
                && !registry
                    .built_in_node_types
                    .contains_key(&node.node_type_name)
            {
                networks.insert(node.node_type_name.clone());
            }
            collect_record_refs_in_node(node, &mut |n, site| {
                if site == RecordRefSite::Schema && is_under(n, direct_alias) && n != direct_alias {
                    records.insert(n.to_string());
                }
            });
        });
    }
    (networks, records)
}

/// The `uses` table of direct mount `direct_alias` (D13): for every name under
/// it that a local node refers to, the interface **in memory** when the name
/// resolves, else the entry carried over verbatim from the mount's
/// `stored_uses`. Names neither resolvable nor stored are omitted.
pub fn collect_used_interfaces(registry: &NodeTypeRegistry, direct_alias: &str) -> UsedInterfaces {
    let stored = registry
        .library_links
        .get(direct_alias)
        .map(|m| m.stored_uses.clone())
        .unwrap_or_default();
    let key = |full: &str| full[direct_alias.len() + 1..].to_string();
    let (networks, records) = referenced_names(registry, direct_alias);
    let mut out = UsedInterfaces::default();
    for full in networks {
        let k = key(&full);
        if let Some(network) = registry.node_networks.get(&full) {
            out.networks.insert(k, network_interface(network));
        } else if let Some(entry) = stored.networks.get(&k) {
            out.networks.insert(k, entry.clone());
        }
    }
    for full in records {
        let k = key(&full);
        if let Some(def) = registry.lookup_record_type_def(&full) {
            out.records.insert(k, record_interface(def));
        } else if let Some(entry) = stored.records.get(&k) {
            out.records.insert(k, entry.clone());
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Users of a mount (unlink refusal, D2)
// ---------------------------------------------------------------------------

/// Every place in the **local** content that refers to a name under
/// `mount_path` by any reference kind of §8 — instances, record schemas, and
/// `DataType`s in node data, local network signatures and local record defs.
/// Empty = the mount can be unlinked without freezing anything.
pub fn mount_users(registry: &NodeTypeRegistry, mount_path: &str) -> Vec<String> {
    let under = |n: &str| is_under(n, mount_path);
    let mentions = |t: &DataType| {
        let mut hit = false;
        walk_data_type_record_names(t, &mut |n| hit |= under(n));
        hit
    };
    let mut users = BTreeSet::new();
    for (name, network) in &registry.node_networks {
        if registry.library_links.mount_containing(name).is_some() {
            continue;
        }
        if network
            .node_type
            .parameters
            .iter()
            .any(|p| mentions(&p.data_type))
            || network
                .node_type
                .output_pins
                .iter()
                .any(|p| p.fixed_type().is_some_and(&mentions))
        {
            users.insert(format!("network `{}` (interface)", name));
        }
        walk_all_nodes(network, &mut |node| {
            let mut hit = under(&node.node_type_name);
            collect_record_refs_in_node(node, &mut |n, _| hit |= under(n));
            if hit {
                let node_name = node
                    .custom_name
                    .clone()
                    .unwrap_or_else(|| node.id.to_string());
                users.insert(format!("network `{}`: node `{}`", name, node_name));
            }
        });
    }
    for (name, def) in &registry.record_type_defs {
        if registry.library_links.mount_containing(name).is_some() {
            continue;
        }
        if def.fields.iter().any(|f| mentions(&f.data_type)) {
            users.insert(format!("record `{}`", name));
        }
    }
    users.into_iter().collect()
}

// ---------------------------------------------------------------------------
// Prefixing, mounting, unmounting
// ---------------------------------------------------------------------------

/// Prefixes every user name in `temp` with `alias` (D1/D4): networks and the
/// `node_type_name`s referring to them, record defs and every `Named` / schema
/// reference to them, empty-folder markers, and the registry's own mounts
/// (which become nested mounts). Built-in names are left alone; every other
/// name is prefixed even when it does not resolve inside the library, so that
/// it can never resolve against the host by accident.
pub fn prefix_registry(temp: &mut NodeTypeRegistry, alias: &str) {
    let p = |n: &str| format!("{}.{}", alias, n);

    // Networks and instance references.
    let networks = std::mem::take(&mut temp.node_networks);
    for (name, mut network) in networks {
        network.node_type.name = p(&name);
        let built_ins = &temp.built_in_node_types;
        walk_all_nodes_mut(&mut network, &mut |node| {
            if !built_ins.contains_key(&node.node_type_name) {
                node.node_type_name = p(&node.node_type_name);
            }
        });
        temp.node_networks.insert(p(&name), network);
    }

    // Record defs (keys and names), then every reference to any user record.
    let defs = std::mem::take(&mut temp.record_type_defs);
    for (name, mut def) in defs {
        def.name = p(&name);
        temp.record_type_defs.insert(p(&name), def);
    }
    let built_in_records: BTreeSet<String> =
        temp.built_in_record_type_defs.keys().cloned().collect();
    crate::node_type_registry::rewrite_record_names_in_registry_with(temp, &mut |name| {
        if !name.is_empty() && !built_in_records.contains(name.as_str()) {
            *name = p(name);
        }
    });

    // Folders.
    temp.folders = std::mem::take(&mut temp.folders)
        .into_iter()
        .map(|f| p(&f))
        .collect();

    // Mounts become nested mounts.
    let mounts: Vec<LibraryMount> = temp.library_links.mounts.values().cloned().collect();
    temp.library_links.mounts.clear();
    // The `uses` tables of the library's own links spell types in the
    // library's namespace; they describe prefixed names from now on.
    let prefix_type = |s: &mut String| {
        if let Ok(mut t) = DataType::from_string(s) {
            crate::data_type::walk_data_type_record_names_mut(&mut t, &mut |name: &mut String| {
                if !name.is_empty() && !built_in_records.contains(name.as_str()) {
                    *name = p(name);
                }
            });
            *s = t.to_string();
        }
    };
    for mut m in mounts {
        let uses = &mut m.stored_uses;
        for entry in uses.networks.values_mut() {
            entry
                .params
                .iter_mut()
                .for_each(|x| prefix_type(&mut x.data_type));
            entry
                .outputs
                .iter_mut()
                .for_each(|x| prefix_type(&mut x.data_type));
        }
        for entry in uses.records.values_mut() {
            entry
                .fields
                .iter_mut()
                .for_each(|x| prefix_type(&mut x.data_type));
        }
        m.mount_path = p(&m.mount_path);
        m.name_only_param_ids = std::mem::take(&mut m.name_only_param_ids)
            .into_iter()
            .map(|(k, v)| (p(&k), v))
            .collect();
        m.parent = Some(match m.parent {
            Some(parent) => p(&parent),
            None => alias.to_string(),
        });
        temp.library_links.insert(m);
    }
}

/// Everything that was under a mount path, detached from a registry.
#[derive(Clone)]
pub struct DetachedMount {
    pub networks: Vec<(String, NodeNetwork)>,
    pub record_type_defs: Vec<RecordTypeDef>,
    pub folders: Vec<String>,
    pub mounts: Vec<LibraryMount>,
}

impl std::fmt::Debug for DetachedMount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DetachedMount")
            .field(
                "networks",
                &self.networks.iter().map(|(n, _)| n).collect::<Vec<_>>(),
            )
            .field("record_type_defs", &self.record_type_defs.len())
            .field("folders", &self.folders)
            .field("mounts", &self.mounts)
            .finish()
    }
}

/// Removes everything at or under `mount_path` — networks, record defs,
/// folders, mount records (nested ones included) — and returns it.
pub fn unmount(registry: &mut NodeTypeRegistry, mount_path: &str) -> DetachedMount {
    let net_keys: Vec<String> = registry
        .node_networks
        .keys()
        .filter(|k| is_under(k, mount_path))
        .cloned()
        .collect();
    let mut networks: Vec<(String, NodeNetwork)> = net_keys
        .into_iter()
        .filter_map(|k| registry.node_networks.remove(&k).map(|n| (k, n)))
        .collect();
    networks.sort_by(|a, b| a.0.cmp(&b.0));
    let def_keys: Vec<String> = registry
        .record_type_defs
        .keys()
        .filter(|k| is_under(k, mount_path))
        .cloned()
        .collect();
    let mut record_type_defs: Vec<RecordTypeDef> = def_keys
        .into_iter()
        .filter_map(|k| registry.record_type_defs.remove(&k))
        .collect();
    record_type_defs.sort_by(|a, b| a.name.cmp(&b.name));
    let folders: Vec<String> = registry
        .folders
        .iter()
        .filter(|f| is_under(f, mount_path))
        .cloned()
        .collect();
    for f in &folders {
        registry.folders.remove(f);
    }
    let mounts = registry.library_links.remove_subtree(mount_path);
    DetachedMount {
        networks,
        record_type_defs,
        folders,
        mounts,
    }
}

/// Puts a detached mount back (the inverse of [`unmount`]).
pub fn restore_detached(registry: &mut NodeTypeRegistry, detached: &DetachedMount) {
    for (name, network) in &detached.networks {
        registry.node_networks.insert(name.clone(), network.clone());
    }
    for def in &detached.record_type_defs {
        registry
            .record_type_defs
            .insert(def.name.clone(), def.clone());
    }
    for f in &detached.folders {
        registry.folders.insert(f.clone());
    }
    for m in &detached.mounts {
        registry.library_links.insert(m.clone());
    }
}

/// `name` moved from under `from` to under `to`, or `None` when it does not
/// lie under `from`.
pub fn reprefixed(name: &str, from: &str, to: &str) -> Option<String> {
    is_under(name, from).then(|| format!("{}{}", to, &name[from.len()..]))
}

/// Moves every name at or under `from` to lie under `to` instead, wherever a
/// name can appear in the registry: network keys and `node_type_name`s,
/// record defs and every record reference (`Named` types, schema strings),
/// folders, mount records (path, parent, recorded types, healed-id keys), the
/// data-file watches' owners, and backtick references in descriptions and
/// comments. Unresolved references (frozen nodes) move too — they are names,
/// whether or not anything defines them. Frozen nodes get their recorded
/// layouts again afterwards (the record rewrite rebuilds record nodes' caches).
///
/// A bijection as long as nothing lay under `to` before, so
/// `rename_prefix(r, to, from)` is its exact inverse — *Rename alias…* and its
/// undo (`doc/design_library_linking.md` D3). Callers check that; this does not.
pub fn rename_prefix(registry: &mut NodeTypeRegistry, from: &str, to: &str) {
    let map = |name: &str| reprefixed(name, from, to);

    // Networks: keys and names first, so that the record rewrite below finds
    // every network under its final name.
    let keys: Vec<String> = registry
        .node_networks
        .keys()
        .filter(|k| is_under(k, from))
        .cloned()
        .collect();
    let moved: Vec<(String, NodeNetwork)> = keys
        .into_iter()
        .filter_map(|k| registry.node_networks.remove(&k).map(|n| (k, n)))
        .collect();
    for (key, mut network) in moved {
        let new_key = map(&key).expect("filtered above");
        network.node_type.name = new_key.clone();
        registry.node_networks.insert(new_key, network);
    }

    // Instance references and backtick references, in every network.
    let (old_dot, new_dot) = (format!("`{}.", from), format!("`{}.", to));
    let (old_whole, new_whole) = (format!("`{}`", from), format!("`{}`", to));
    let rewrite_text = |text: &mut String| {
        if text.contains(&old_dot) || text.contains(&old_whole) {
            *text = text
                .replace(&old_dot, &new_dot)
                .replace(&old_whole, &new_whole);
        }
    };
    let NodeTypeRegistry {
        built_in_node_types,
        node_networks,
        ..
    } = &mut *registry;
    for network in node_networks.values_mut() {
        rewrite_text(&mut network.node_type.description);
        if let Some(summary) = network.node_type.summary.as_mut() {
            rewrite_text(summary);
        }
        walk_all_nodes_mut(network, &mut |node| {
            if !built_in_node_types.contains_key(&node.node_type_name)
                && let Some(new_name) = map(&node.node_type_name)
            {
                node.node_type_name = new_name;
            }
            if let Some(comment) = node
                .data
                .as_any_mut()
                .downcast_mut::<crate::nodes::comment::CommentData>()
            {
                rewrite_text(&mut comment.label);
                rewrite_text(&mut comment.text);
            }
        });
    }

    // Record defs, then every reference to them.
    let def_keys: Vec<String> = registry
        .record_type_defs
        .keys()
        .filter(|k| is_under(k, from))
        .cloned()
        .collect();
    let defs: Vec<RecordTypeDef> = def_keys
        .into_iter()
        .filter_map(|k| registry.record_type_defs.remove(&k))
        .collect();
    for mut def in defs {
        def.name = map(&def.name).expect("filtered above");
        registry.record_type_defs.insert(def.name.clone(), def);
    }
    crate::node_type_registry::rewrite_record_names_in_registry_with(registry, &mut |name| {
        if let Some(new_name) = map(name) {
            *name = new_name;
        }
    });

    // Folders.
    registry.folders = std::mem::take(&mut registry.folders)
        .into_iter()
        .map(|f| map(&f).unwrap_or(f))
        .collect();

    // Mount records. Recorded types are spelled in the importing file's
    // namespace, which is the host's for a direct mount and the prefixed one
    // for a nested mount, so every record's types are rewritten.
    let map_type = |s: &mut String| {
        if let Ok(mut t) = DataType::from_string(s) {
            let mut changed = false;
            crate::data_type::walk_data_type_record_names_mut(&mut t, &mut |name: &mut String| {
                if let Some(new_name) = map(name) {
                    *name = new_name;
                    changed = true;
                }
            });
            if changed {
                *s = t.to_string();
            }
        }
    };
    let mounts: Vec<LibraryMount> = std::mem::take(&mut registry.library_links.mounts)
        .into_values()
        .collect();
    for mut m in mounts {
        for entry in m.stored_uses.networks.values_mut() {
            entry
                .params
                .iter_mut()
                .for_each(|p| map_type(&mut p.data_type));
            entry
                .outputs
                .iter_mut()
                .for_each(|o| map_type(&mut o.data_type));
        }
        for entry in m.stored_uses.records.values_mut() {
            entry
                .fields
                .iter_mut()
                .for_each(|f| map_type(&mut f.data_type));
        }
        if let Some(new_path) = map(&m.mount_path) {
            if m.parent.is_none() {
                m.alias = new_path.clone();
            }
            m.mount_path = new_path;
            m.parent = m.parent.map(|p| map(&p).unwrap_or(p));
            m.name_only_param_ids = std::mem::take(&mut m.name_only_param_ids)
                .into_iter()
                .map(|(k, v)| (map(&k).unwrap_or(k), v))
                .collect();
        }
        registry.library_links.insert(m);
    }

    // Data-file watches are keyed by the owning mount.
    registry.library_links.data_files = std::mem::take(&mut registry.library_links.data_files)
        .into_iter()
        .map(|(mut key, watch)| {
            key.owner = key.owner.map(|o| map(&o).unwrap_or(o));
            (key, watch)
        })
        .collect();

    reinstall_recorded_layouts(registry);
}

/// Installs the recorded layout of every frozen node of every network again,
/// each with its own owner mount (see [`install_recorded_layout`]).
pub fn reinstall_recorded_layouts(registry: &mut NodeTypeRegistry) {
    if registry.library_links.is_empty() {
        return;
    }
    let names: Vec<String> = registry.node_networks.keys().cloned().collect();
    for name in names {
        let owner = owner_mount(registry, &name);
        if let Some(mut network) = registry.node_networks.remove(&name) {
            install_recorded_layouts(&mut network, registry, owner.as_deref());
            registry.node_networks.insert(name, network);
        }
    }
}

/// Every place — in the local content *and* inside the mount — that refers to
/// a name under `mount_path` which does not resolve: frozen nodes, and local
/// network signatures or record defs whose types name such a record. Such a
/// reference is safe only while the name lies under a mount (§8); *Make local
/// copy* would turn it into an ordinary unknown name that repair passes
/// realign, so it refuses while this is non-empty.
pub fn unresolved_refs_under(registry: &NodeTypeRegistry, mount_path: &str) -> Vec<String> {
    let under_unresolved = |name: &str| {
        is_under(name, mount_path)
            && !registry.node_networks.contains_key(name)
            && !registry.built_in_node_types.contains_key(name)
            && registry.lookup_record_type_def(name).is_none()
    };
    let type_hit = |t: &DataType| {
        let mut hit = false;
        walk_data_type_record_names(t, &mut |n| hit |= under_unresolved(n));
        hit
    };
    let mut out = BTreeSet::new();
    let mut names: Vec<&String> = registry.node_networks.keys().collect();
    names.sort();
    for name in names {
        let network = &registry.node_networks[name];
        if network
            .node_type
            .parameters
            .iter()
            .any(|p| type_hit(&p.data_type))
            || network
                .node_type
                .output_pins
                .iter()
                .any(|p| p.fixed_type().is_some_and(&type_hit))
        {
            out.insert(format!("network `{}` (interface)", name));
        }
        walk_all_nodes(network, &mut |node| {
            let mut hit = under_unresolved(&node.node_type_name);
            collect_record_refs_in_node(node, &mut |n, _| hit |= under_unresolved(n));
            if hit {
                let node_name = node
                    .custom_name
                    .clone()
                    .unwrap_or_else(|| node.id.to_string());
                out.insert(format!("network `{}`: node `{}`", name, node_name));
            }
        });
    }
    for (name, def) in &registry.record_type_defs {
        if def.fields.iter().any(|f| type_hit(&f.data_type)) {
            out.insert(format!("record `{}`", name));
        }
    }
    out.into_iter().collect()
}

/// Where a mount's file lives and what the importing file recorded about it.
#[derive(Clone, Debug)]
pub struct ImportSpec {
    pub alias: String,
    pub rel_path: String,
    pub stored_hash: Option<String>,
    pub stored_uses: UsedInterfaces,
}

/// Mounts one import of the file `importing_file` into `registry` as a direct
/// mount under `spec.alias`, recursing into the library's own imports. Never
/// fails: a problem becomes the mount's status (`Missing`, `Error`, `Cycle`)
/// and nothing is put under the mount path. `stack` holds the canonical paths
/// of every file being loaded above this one (cycle detection, D4).
pub fn mount_library(
    registry: &mut NodeTypeRegistry,
    spec: &ImportSpec,
    importing_file: &Path,
    stack: &mut Vec<PathBuf>,
) -> MountStatus {
    let fs = registry.library_links.fs();
    let importing_dir = importing_file.parent().unwrap_or(Path::new(""));
    let mut mount = LibraryMount {
        mount_path: spec.alias.clone(),
        alias: spec.alias.clone(),
        rel_path: spec.rel_path.clone(),
        abs_path: PathBuf::new(),
        parent: None,
        status: MountStatus::Missing,
        loaded: None,
        last_seen: None,
        stored_hash: spec.stored_hash.clone(),
        stored_uses: spec.stored_uses.clone(),
        name_only_param_ids: BTreeMap::new(),
    };

    let status = (|| -> MountStatus {
        let joined = match lexical_join(importing_dir, &spec.rel_path) {
            Ok(p) => p,
            Err(e) => return MountStatus::Error(e),
        };
        mount.abs_path = joined.clone();
        let canonical = match fs.canonicalize(&joined) {
            Ok(p) => p,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return MountStatus::Missing,
            Err(e) => {
                return MountStatus::Error(format!("cannot resolve '{}': {}", spec.rel_path, e));
            }
        };
        mount.abs_path = canonical.clone();
        if stack.contains(&canonical) {
            return MountStatus::Cycle;
        }
        // Read once: the stamp and hash describe exactly the bytes parsed.
        // The mtime is taken *before* the read, so a change landing between
        // the two leaves a stamp older than the file, which the next check
        // re-hashes (D7) — never one that makes newer bytes look loaded.
        let mtime = fs.stat(&canonical).ok().and_then(|m| m.mtime);
        let bytes = match fs.read(&canonical) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return MountStatus::Missing,
            Err(e) => return MountStatus::Error(format!("cannot read '{}': {}", spec.rel_path, e)),
        };
        let stamp = FileStamp::of_bytes(&bytes, mtime);
        // Seen, whether or not it parses: an unparseable file is not tried
        // again until it changes (D7).
        mount.last_seen = Some(stamp.clone());

        let mut temp = NodeTypeRegistry::new();
        temp.library_links.set_fs(fs.clone());
        stack.push(canonical.clone());
        let loaded =
            crate::serialization::node_networks_serialization::load_node_networks_from_bytes(
                &mut temp,
                &bytes,
                &joined.to_string_lossy(),
                stack,
            );
        stack.pop();
        if let Err(e) = loaded {
            return MountStatus::Error(format!("cannot load '{}': {}", spec.rel_path, e));
        }

        // Load-time heals that must see the ids the library will really have
        // before anything matches by `param_id` against it (§5.1).
        for (name, network) in temp.node_networks.iter_mut() {
            let healed: BTreeSet<u64> =
                crate::network_validator::dedupe_param_ids_in_network(network)
                    .into_iter()
                    .map(|fix| fix.old_param_id)
                    .collect();
            if !healed.is_empty() {
                mount
                    .name_only_param_ids
                    .insert(format!("{}.{}", spec.alias, name), healed);
            }
        }
        // Validate the library as its own document would be on open, before
        // anything reconciles against it: a `.cnnd` can carry stale
        // interfaces (a record def edit refreshes only the networks that get
        // validated — `split`'s output pins can still list the old fields), and
        // the importing file must match its recorded interfaces against the
        // interfaces the library really has, or the post-mount validation
        // moves them a second time, silently (D13).
        validate_all_networks(&mut temp);
        // A linked network starts from the default framing (§5.4).
        for network in temp.node_networks.values_mut() {
            network.camera_settings = None;
            network.canvas_viewport = None;
        }

        prefix_registry(&mut temp, &spec.alias);
        let networks: Vec<(String, NodeNetwork)> = temp.node_networks.drain().collect();
        for (name, def) in temp.record_type_defs.drain() {
            registry.record_type_defs.insert(name, def);
        }
        for f in std::mem::take(&mut temp.folders) {
            registry.folders.insert(f);
        }
        for m in temp.library_links.mounts.values() {
            registry.library_links.insert(m.clone());
        }
        // Frozen nodes of the library get their recorded layouts again, now
        // that names and recorded types carry the prefix (prefixing rebuilt
        // record nodes' layouts from the — missing — defs).
        // Everything is inserted first, so that only names that really do not
        // resolve count as frozen.
        let names: Vec<String> = networks.iter().map(|(n, _)| n.clone()).collect();
        registry.node_networks.extend(networks);
        for name in names {
            let owner = owner_of_prefixed(registry, &name, &spec.alias);
            if let Some(mut network) = registry.node_networks.remove(&name) {
                install_recorded_layouts(&mut network, registry, owner.as_deref());
                registry.node_networks.insert(name, network);
            }
        }
        mount.loaded = Some(stamp.clone());
        mount.last_seen = Some(stamp);
        MountStatus::Loaded
    })();

    mount.status = status.clone();
    registry.library_links.insert(mount);
    status
}

/// Where the mount record `mount_path` points now: its `rel_path` joined onto
/// the folder of the file that imports it (the design file for a direct
/// mount, the parent library for a nested one), canonical when the file
/// exists — exactly what `mount_library` computes. `None` for a cycle, or
/// when the importing file is unknown.
pub fn resolved_mount_path(registry: &NodeTypeRegistry, mount_path: &str) -> Option<PathBuf> {
    let mount = registry.library_links.get(mount_path)?;
    if mount.status == MountStatus::Cycle {
        return None;
    }
    let importing: PathBuf = match &mount.parent {
        None => PathBuf::from(registry.design_file_name.as_ref()?),
        Some(parent) => registry.library_links.get(parent)?.abs_path.clone(),
    };
    let joined = lexical_join(importing.parent().unwrap_or(Path::new("")), &mount.rel_path).ok()?;
    let fs = registry.library_links.fs();
    Some(fs.canonicalize(&joined).unwrap_or(joined))
}

/// Re-resolves every mount's `abs_path` from its `rel_path` chain, parents
/// before children (a parent's mount path sorts before its children's). A
/// mount whose file moved — the design was saved into another folder
/// (D7, D11), or an undo restored a record resolved against an earlier folder
/// — forgets the mtime of its last sighting, so the next check hashes the file
/// it now points at instead of trusting a stat of another file. Returns the
/// mount paths that moved.
pub fn relocate_mounts(registry: &mut NodeTypeRegistry) -> Vec<String> {
    let paths: Vec<String> = registry
        .library_links
        .iter()
        .map(|m| m.mount_path.clone())
        .collect();
    let mut moved = Vec::new();
    for mount_path in paths {
        let Some(now) = resolved_mount_path(registry, &mount_path) else {
            continue;
        };
        let mount = registry
            .library_links
            .get_mut(&mount_path)
            .expect("listed above");
        if mount.abs_path != now {
            mount.abs_path = now;
            if let Some(seen) = mount.last_seen.as_mut() {
                seen.mtime = None;
            }
            moved.push(mount_path);
        }
    }
    moved
}

/// [`owner_mount`] of a network being mounted under `alias`, whose own mount
/// record is not inserted yet: its nested mount if it lies in one, else the
/// mount `alias` itself.
fn owner_of_prefixed(registry: &NodeTypeRegistry, name: &str, alias: &str) -> Option<String> {
    registry
        .library_links
        .mount_containing(name)
        .map(|m| m.mount_path.clone())
        .filter(|p| is_under(p, alias))
        .or_else(|| Some(alias.to_string()))
}

/// Validates every network of `registry` in dependency order — what a load
/// does after inserting everything. Used after a mount or unmount changes the
/// set of resolvable names.
pub fn validate_all_networks(registry: &mut NodeTypeRegistry) {
    let order = registry.get_networks_in_dependency_order();
    for name in order {
        if let Some(mut network) = registry.node_networks.remove(&name) {
            crate::network_validator::validate_network(&mut network, registry, None);
            registry.node_networks.insert(name, network);
        }
    }
}

// ---------------------------------------------------------------------------
// Tripwire (§6)
// ---------------------------------------------------------------------------

/// Canonical JSON of every network, record def and folder under `mount_path`,
/// view state stripped, plus the nested mount paths. Wires are recorded by
/// exact pin index, so a moved wire changes it.
pub fn mount_fingerprint(registry: &NodeTypeRegistry, mount_path: &str) -> String {
    let mut names: Vec<&String> = registry
        .node_networks
        .keys()
        .filter(|k| is_under(k, mount_path))
        .collect();
    names.sort();
    let mut networks = serde_json::Map::new();
    for name in names {
        let mut clone = registry.node_networks[name].clone();
        clone.camera_settings = None;
        clone.canvas_viewport = None;
        let value =
            crate::serialization::node_networks_serialization::node_network_to_serializable(
                &mut clone,
                &registry.built_in_node_types,
                None,
                false,
            )
            .ok()
            .and_then(|s| serde_json::to_value(s).ok())
            .unwrap_or(serde_json::Value::Null);
        networks.insert(name.clone(), value);
    }
    let mut defs: Vec<&RecordTypeDef> = registry
        .record_type_defs
        .values()
        .filter(|d| is_under(&d.name, mount_path))
        .collect();
    defs.sort_by(|a, b| a.name.cmp(&b.name));
    let folders: Vec<&String> = registry
        .folders
        .iter()
        .filter(|f| is_under(f, mount_path))
        .collect();
    let mounts: Vec<&String> = registry
        .library_links
        .mounts
        .keys()
        .filter(|k| is_under(k, mount_path))
        .collect();
    let value = serde_json::json!({
        "networks": networks,
        "record_type_defs": defs,
        "folders": folders,
        "mounts": mounts,
    });
    serde_json::to_string_pretty(&value).unwrap_or_default()
}
