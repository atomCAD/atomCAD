//! Shared oracles for the library-linking tests
//! (`doc/design_library_linking.md` §11.1). Every library-linking test uses
//! these rather than ad-hoc assertions, and each oracle has a negative control
//! in `library_links_test.rs` proving it can fail.
//!
//! - **O1** [`local_fingerprint`] — the local content, as saved, view state
//!   stripped.
//! - **O2** [`wire_ledger`] / [`check_wire_ledger`] — every host wire keyed by
//!   identity; a wire that is gone and not reported is a silent loss.
//! - **O3** [`DiskTripwire`] — no file changed that the test did not allow.
//! - **O4** [`check_save_reopen_identity`] — save → reopen gives the same O1,
//!   and save → load → save is byte-identical.
//! - **O5** [`check_undo_inverse`] — undo restores O1 and every mount
//!   fingerprint exactly; redo restores the post-op state exactly.
//! - **O6** [`check_invariants`] — the Phase 0 document checker finds nothing
//!   fatal.

#![allow(dead_code)]

use atomcad_structure_designer::invariants::check_document_invariants;
use atomcad_structure_designer::library_links::{is_frozen, mount_fingerprint};
use atomcad_structure_designer::node_network::{NodeNetwork, SourcePin};
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::serialization::node_networks_serialization::serialize_registry_to_string;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_test_support::fixture_path;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Workspace helpers
// ---------------------------------------------------------------------------

/// Copies `fixtures/library_linking/` into a fresh temp dir. Every test that
/// opens a fixture works on such a copy, never on the committed files.
pub fn fixture_workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    copy_dir(&fixture_path("library_linking"), dir.path());
    dir
}

pub fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// A fresh designer with `path` loaded.
pub fn open(path: &Path) -> StructureDesigner {
    let mut d = StructureDesigner::new();
    d.load_node_networks(&path.to_string_lossy())
        .unwrap_or_else(|e| panic!("load {}: {}", path.display(), e));
    d
}

/// The text a save of `d` would write, with the design folder `dir`.
pub fn saved_text(d: &mut StructureDesigner, dir: Option<&Path>) -> String {
    let dir = dir.map(|p| p.to_string_lossy().to_string());
    let rules = d.cli_access_rules.clone();
    let mode = d.direct_editing_mode;
    serialize_registry_to_string(&mut d.node_type_registry, dir.as_deref(), mode, &rules)
        .expect("serialize")
}

fn design_dir(d: &StructureDesigner) -> Option<PathBuf> {
    d.file_path
        .as_ref()
        .and_then(|p| Path::new(p).parent().map(|p| p.to_path_buf()))
}

// ---------------------------------------------------------------------------
// O1 — local fingerprint
// ---------------------------------------------------------------------------

fn strip_view_state(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("camera_settings");
            map.remove("canvas_viewport");
            for v in map.values_mut() {
                strip_view_state(v);
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(strip_view_state),
        _ => {}
    }
}

/// O1: the canonical `.cnnd` JSON of the **local** content (the D5 filter),
/// view state stripped, import hashes dropped (they describe the libraries,
/// not the user's work). Wires appear by exact pin index, so a moved wire
/// changes it.
pub fn local_fingerprint(d: &mut StructureDesigner) -> String {
    let dir = design_dir(d);
    let text = saved_text(d, dir.as_deref());
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    strip_view_state(&mut value);
    if let Some(imports) = value.get_mut("imports").and_then(|v| v.as_array_mut()) {
        for import in imports {
            if let Some(obj) = import.as_object_mut() {
                obj.remove("hash");
            }
        }
    }
    serde_json::to_string_pretty(&value).unwrap()
}

// ---------------------------------------------------------------------------
// O2 — wire ledger
// ---------------------------------------------------------------------------

/// Which input a wire feeds, by identity where there is one.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Slot {
    /// A parameter / record field carrying a stable id.
    Id(u64),
    /// A built-in pin without an id — its name is stable.
    Name(String),
    /// A frozen node (§8), or a node whose type does not resolve: position.
    Pos(usize),
    /// A zone-output pin of an HOF, by index.
    ZoneOut(usize),
}

/// One wire of the host, keyed by identity (the O2 key of §11.1).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WireEntry {
    pub network: String,
    pub scope: Vec<u64>,
    pub dest: u64,
    pub slot: Slot,
    pub source: u64,
    pub source_pin: String,
    pub depth: u8,
}

fn pin_label(pin: &SourcePin) -> String {
    match pin {
        SourcePin::NodeOutput { pin_index } => format!("out{}", pin_index),
        SourcePin::ZoneInput { pin_index } => format!("zin{}", pin_index),
    }
}

/// The `param_id` of the parameter named `name` of the custom network `node`
/// instantiates, read from the network's parameter nodes. A network restored
/// from an undo snapshot carries no ids in its interface until it is next
/// validated; the ids on the parameter nodes are the stable ones.
fn param_id_by_name(
    registry: &NodeTypeRegistry,
    node: &atomcad_structure_designer::node_network::Node,
    name: &str,
) -> Option<u64> {
    use atomcad_structure_designer::nodes::parameter::ParameterData;
    let network = registry.node_networks.get(&node.node_type_name)?;
    network.nodes.values().find_map(|n| {
        let p = n.data.as_any_ref().downcast_ref::<ParameterData>()?;
        (p.param_name == name).then_some(p.param_id).flatten()
    })
}

fn ledger_scope(
    registry: &NodeTypeRegistry,
    network_name: &str,
    network: &NodeNetwork,
    scope: &[u64],
    out: &mut BTreeSet<WireEntry>,
) {
    for node in network.nodes.values() {
        let frozen = is_frozen(node, registry);
        let node_type = if frozen {
            None
        } else {
            registry.get_node_type_for_node(node)
        };
        for (i, arg) in node.arguments.iter().enumerate() {
            let slot = match node_type.and_then(|t| t.parameters.get(i)) {
                Some(p) => match p.id.or_else(|| param_id_by_name(registry, node, &p.name)) {
                    Some(id) => Slot::Id(id),
                    None => Slot::Name(p.name.clone()),
                },
                None => Slot::Pos(i),
            };
            for w in &arg.incoming_wires {
                out.insert(WireEntry {
                    network: network_name.to_string(),
                    scope: scope.to_vec(),
                    dest: node.id,
                    slot: slot.clone(),
                    source: w.source_node_id,
                    source_pin: pin_label(&w.source_pin),
                    depth: w.source_scope_depth,
                });
            }
        }
        for (i, arg) in node.zone_output_arguments.iter().enumerate() {
            for w in &arg.incoming_wires {
                out.insert(WireEntry {
                    network: network_name.to_string(),
                    scope: scope.to_vec(),
                    dest: node.id,
                    slot: Slot::ZoneOut(i),
                    source: w.source_node_id,
                    source_pin: pin_label(&w.source_pin),
                    depth: w.source_scope_depth,
                });
            }
        }
        if let Some(body) = node.zone.as_deref() {
            let mut child = scope.to_vec();
            child.push(node.id);
            ledger_scope(registry, network_name, body, &child, out);
        }
    }
}

/// O2: every wire of every local network (bodies included).
pub fn wire_ledger(d: &StructureDesigner) -> BTreeSet<WireEntry> {
    let registry = &d.node_type_registry;
    let mut out = BTreeSet::new();
    for (name, network) in &registry.node_networks {
        if registry.library_links.mount_containing(name).is_some() {
            continue;
        }
        ledger_scope(registry, name, network, &[], &mut out);
    }
    out
}

/// O2: every wire of `before` is in `after` or in `reported`, and no wire
/// appeared. A wire re-pointed to the wrong input shows up as one loss plus
/// one appearance.
pub fn check_wire_ledger(
    before: &BTreeSet<WireEntry>,
    after: &BTreeSet<WireEntry>,
    reported: &BTreeSet<WireEntry>,
) -> Result<(), String> {
    let lost: Vec<&WireEntry> = before
        .iter()
        .filter(|w| !after.contains(*w) && !reported.contains(*w))
        .collect();
    let appeared: Vec<&WireEntry> = after.iter().filter(|w| !before.contains(*w)).collect();
    if lost.is_empty() && appeared.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "wire ledger: {} silent loss(es) {:#?}, {} appeared {:#?}",
            lost.len(),
            lost,
            appeared.len(),
            appeared
        ))
    }
}

// ---------------------------------------------------------------------------
// O3 — disk tripwire
// ---------------------------------------------------------------------------

/// O3: hashes every file under a root; [`DiskTripwire::check_only_changed`]
/// then reports every file created, deleted or modified outside an allowed
/// list.
pub struct DiskTripwire {
    root: PathBuf,
    hashes: BTreeMap<PathBuf, String>,
}

fn hash_tree(root: &Path, out: &mut BTreeMap<PathBuf, String>) {
    for entry in std::fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if entry.file_type().unwrap().is_dir() {
            hash_tree(&path, out);
        } else {
            let bytes = std::fs::read(&path).unwrap();
            out.insert(path, blake3::hash(&bytes).to_hex().to_string());
        }
    }
}

impl DiskTripwire {
    pub fn arm(root: &Path) -> Self {
        let mut hashes = BTreeMap::new();
        hash_tree(root, &mut hashes);
        Self {
            root: root.to_path_buf(),
            hashes,
        }
    }

    pub fn check_only_changed(&self, allowed: &[PathBuf]) -> Result<(), String> {
        let mut now = BTreeMap::new();
        hash_tree(&self.root, &mut now);
        let allowed: BTreeSet<&PathBuf> = allowed.iter().collect();
        let mut changed = Vec::new();
        for (path, hash) in &now {
            if self.hashes.get(path) != Some(hash) && !allowed.contains(path) {
                changed.push(format!("changed or created: {}", path.display()));
            }
        }
        for path in self.hashes.keys() {
            if !now.contains_key(path) && !allowed.contains(path) {
                changed.push(format!("deleted: {}", path.display()));
            }
        }
        if changed.is_empty() {
            Ok(())
        } else {
            Err(format!("disk tripwire: {:#?}", changed))
        }
    }
}

// ---------------------------------------------------------------------------
// O4 — persistence
// ---------------------------------------------------------------------------

/// O4: saves `d` over its own design file, reopens that file in a fresh
/// designer, and requires (a) the same O1 and (b) that saving the reopened
/// design writes byte-identical text. The file's previous bytes are put back
/// afterwards, so the oracle leaves the disk as it found it. (Saving under a
/// different name would change what the relative import paths mean — a file
/// linking itself would stop being a cycle.)
pub fn check_save_reopen_identity(d: &mut StructureDesigner) -> Result<(), String> {
    let path = PathBuf::from(d.file_path.clone().ok_or("the design has no file")?);
    let dir = path.parent().map(|p| p.to_path_buf());
    let previous = std::fs::read(&path).ok();
    let first = saved_text(d, dir.as_deref());
    std::fs::write(&path, &first).map_err(|e| e.to_string())?;
    let result = (|| {
        let mut reopened = StructureDesigner::new();
        reopened
            .load_node_networks(&path.to_string_lossy())
            .map_err(|e| format!("reopen: {}", e))?;
        let (a, b) = (local_fingerprint(d), local_fingerprint(&mut reopened));
        if a != b {
            return Err(format!(
                "O4(a): fingerprint changed across save → reopen
--- before
{}
--- after
{}",
                a, b
            ));
        }
        let second = saved_text(&mut reopened, dir.as_deref());
        if first != second {
            return Err(format!(
                "O4(b): save → load → save is not byte-identical
--- first
{}
--- second
{}",
                first, second
            ));
        }
        Ok(())
    })();
    match previous {
        Some(bytes) => {
            let _ = std::fs::write(&path, bytes);
        }
        None => {
            let _ = std::fs::remove_file(&path);
        }
    }
    result
}

// ---------------------------------------------------------------------------
// O5 — undo inverse
// ---------------------------------------------------------------------------

/// Every mount's fingerprint, keyed by mount path.
pub fn mount_fingerprints(d: &StructureDesigner) -> BTreeMap<String, String> {
    let registry = &d.node_type_registry;
    registry
        .library_links
        .iter()
        .map(|m| {
            (
                m.mount_path.clone(),
                mount_fingerprint(registry, &m.mount_path),
            )
        })
        .collect()
}

/// O5: runs `op` (which must push exactly one undo step), undoes it and
/// requires O1 and every mount fingerprint as before; redoes it and requires
/// them as right after `op`.
pub fn check_undo_inverse(
    d: &mut StructureDesigner,
    op: impl FnOnce(&mut StructureDesigner),
) -> Result<(), String> {
    let before = (local_fingerprint(d), mount_fingerprints(d));
    op(d);
    let after = (local_fingerprint(d), mount_fingerprints(d));
    if !d.undo() {
        return Err("O5: nothing to undo".to_string());
    }
    let undone = (local_fingerprint(d), mount_fingerprints(d));
    if undone != before {
        return Err(format!(
            "O5: undo is not the inverse\n--- before\n{:#?}\n--- after undo\n{:#?}",
            before, undone
        ));
    }
    if !d.redo() {
        return Err("O5: nothing to redo".to_string());
    }
    let redone = (local_fingerprint(d), mount_fingerprints(d));
    if redone != after {
        return Err(format!(
            "O5: redo does not restore the post-op state\n--- after op\n{:#?}\n--- after redo\n{:#?}",
            after, redone
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// O6 — document invariants
// ---------------------------------------------------------------------------

/// O6: the Phase 0 document checker reports nothing fatal (frozen nodes are
/// exempt from the argument-count check by design).
pub fn check_invariants(d: &StructureDesigner) -> Result<(), String> {
    let fatal: Vec<_> = check_document_invariants(&d.node_type_registry)
        .into_iter()
        .filter(|v| v.is_fatal())
        .collect();
    if fatal.is_empty() {
        Ok(())
    } else {
        Err(format!("O6: fatal invariant violations {:#?}", fatal))
    }
}

/// Unwraps an oracle result with its message.
#[track_caller]
pub fn ok(result: Result<(), String>) {
    if let Err(e) = result {
        panic!("{}", e);
    }
}
