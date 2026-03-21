# Design: Context Menu for Tree View (Rename + Delete)

## Problem

The tree view for node networks is missing interactive operations that users expect:

1. **Rename** — only available in the flat list view. Forces users to switch views, scroll, rename, switch back. Especially painful with imported libraries.
2. **Delete** — only available via the action bar button, which always targets the *active* network. No way to right-click → Delete a specific network without first activating it. No way to delete an entire namespace (folder) at once.

Both operations should be accessible via **double-click** (rename) and **right-click context menu** (rename + delete) directly in the tree view, for both leaf nodes (actual networks) and namespace nodes (virtual folders).

## Current Architecture

### Naming Convention

Network names use **dot-delimited hierarchical names** (e.g., `Physics.Mechanics.Spring`). The tree view parses these into a visual hierarchy:

```
Physics/              ← namespace node (virtual, not a real network)
  Mechanics/          ← namespace node
    Spring            ← leaf node (actual network: "Physics.Mechanics.Spring")
    Damper            ← leaf node (actual network: "Physics.Mechanics.Damper")
  Optics/
    Lens              ← leaf node (actual network: "Physics.Optics.Lens")
```

Namespaces don't exist as entities — they're derived from network names at display time.

### Flutter Side

- **`lib/structure_designer/namespace_utils.dart`** — `getSegments()`, `getSimpleName()`, `getNamespace()`, `combineQualifiedName()` for parsing qualified names.
- **`node_network_tree_view.dart`** — Builds a tree from flat names using `_buildTreeFromNames()`. Each `_NodeNetworkTreeNode` has `label` (segment), `fullName` (qualified path), `isLeaf`, `children`. Currently supports click-to-activate (leaf) and click-to-expand (namespace), but **no rename or delete UI**.
- **`node_network_list_view.dart`** — Flat list with rename support: `_editingNetworkName` state, `TextEditingController`, double-click and context menu triggers, ESC to cancel, Enter/blur to commit. Calls `model.renameNodeNetwork(oldName, newName)`. No delete in context menu (only via action bar).
- **`node_networks_action_bar.dart`** — Delete button targets the active network. Shows confirmation dialog, then error dialog if deletion fails (e.g., network is referenced by other networks).
- **`structure_designer_model.dart`** — `renameNodeNetwork(oldName, newName)` and `deleteNodeNetwork(networkName)` call the Rust API, then `refreshFromKernel()`.

### Rust Side

**Rename:**
- `structure_designer.rs:724` — `rename_node_network(old_name, new_name) -> bool`: Validates, renames single network, cascades to all references (node types, clipboard, backtick refs, navigation history). Pushes `RenameNetworkCommand`.
- `undo/commands/rename_network.rs` — `RenameNetworkCommand { old_name, new_name }` with symmetric `do_rename()`.

**Delete:**
- `structure_designer.rs:836` — `delete_node_network(network_name) -> Result<(), String>`: Checks for references from other networks (returns error with names if found). Snapshots network, removes from registry, clears navigation/clipboard references. Pushes `DeleteNetworkCommand`.
- `undo/commands/delete_network.rs` — `DeleteNetworkCommand` stores `SerializableNodeNetwork` snapshot + active network state before/after. Undo deserializes and re-adds.

**No batch/prefix operations** exist on the Rust side for either rename or delete.

### API Surface

- `rename_node_network(old_name: &str, new_name: &str) -> bool` (sync FFI)
- `delete_node_network(network_name: &str) -> APIResult` (sync FFI)

## Design

### Context Menu

Both leaf and namespace nodes get a right-click context menu with:

| Item | Leaf Node | Namespace Node |
|------|-----------|----------------|
| **Rename** | Rename this network (edit simple name) | Rename this namespace segment (batch rename all networks under prefix) |
| **Delete** | Delete this network (with confirmation) | Delete all networks under this namespace (with confirmation listing affected networks) |

### Rename: Two Scenarios

#### 1. Leaf Node Rename (no Rust changes needed)

User double-clicks or right-clicks → Rename on a **leaf node** (actual network).

- The text field shows **only the simple name** (last segment), e.g., `Spring` not `Physics.Mechanics.Spring`.
- On commit, Flutter reconstructs the full qualified name: `combineQualifiedName(getNamespace(oldFullName), newSimpleName)`.
- Calls the existing `model.renameNodeNetwork(oldFullName, newFullName)`.

**Edge case**: If the user types a name containing dots (e.g., `Spring.v2`), the network moves into a new sub-namespace. This is consistent with how the list view works.

#### 2. Namespace Node Rename (requires new Rust API)

User double-clicks or right-clicks → Rename on a **namespace node** (virtual folder).

- The text field shows **only the namespace segment label**, e.g., `Mechanics` not `Physics.Mechanics`.
- On commit, Flutter computes the old and new prefix:
  - Old prefix: `Physics.Mechanics` (the `fullName` of the namespace node)
  - New prefix: `Physics.Dynamics` (replace last segment with user input)
- Calls a **new** API: `model.renameNamespace(oldPrefix, newPrefix)`.
- This renames ALL networks whose name starts with `oldPrefix.` — in one atomic operation, one undo step.

**Example**: Renaming `Mechanics` → `Dynamics`:
- `Physics.Mechanics.Spring` → `Physics.Dynamics.Spring`
- `Physics.Mechanics.Damper` → `Physics.Dynamics.Damper`

### Delete: Two Scenarios

#### 3. Leaf Node Delete (no Rust changes needed)

User right-clicks → Delete on a **leaf node**.

- Shows confirmation dialog: `Delete network "Physics.Mechanics.Spring"?`
- Calls existing `model.deleteNodeNetwork(fullName)`.
- On error (network referenced by others), shows error dialog with referencing network names (same as action bar behavior).

This is the same as the action bar delete, just triggered from context menu on a specific network rather than always targeting the active one.

#### 4. Namespace Node Delete (requires new Rust API)

User right-clicks → Delete on a **namespace node** (virtual folder).

- Shows confirmation dialog listing all affected networks:
  ```
  Delete namespace "Physics.Mechanics" and all 2 networks within it?

  Networks to be deleted:
  • Physics.Mechanics.Spring
  • Physics.Mechanics.Damper
  ```
- Calls a **new** API: `model.deleteNamespace(prefix)`.
- This deletes ALL networks whose name starts with `prefix.` — in one atomic operation, one undo step.
- If any network under the prefix is referenced by a network *outside* the prefix, the entire operation is rejected with an error listing the blocking references.

**Note**: Double-click on namespace nodes is reserved for rename, not delete. Delete is only in the context menu (it's destructive and should require deliberate action).

## Rust Changes

### Shared Helpers (Refactoring)

#### Understanding the Two Execution Contexts

`UndoContext` only exposes `node_type_registry` and `active_network_name` — it does **not** have `navigation_history` or `clipboard`. This is by design (avoiding borrow conflicts with `UndoStack`). The undo context is a strict subset of what the main `StructureDesigner` method touches, so shared helpers must operate on exactly the `UndoContext`-level fields.

#### Rename: `apply_rename_core`

Extract into `rust/src/structure_designer/undo/commands/rename_helpers.rs`:

```rust
/// Core rename logic shared between single rename and namespace rename,
/// in both main-method and undo-command contexts.
///
/// Handles: registry move, active name update, node type reference cascade,
/// backtick reference cascade.
///
/// Does NOT handle: validation, navigation history, clipboard, dirty/refresh, undo push.
pub fn apply_rename_core(
    registry: &mut NodeTypeRegistry,
    active_name: &mut Option<String>,
    old_name: &str,
    new_name: &str,
)
```

**Callers (4 total):**

1. **`rename_node_network` (main method)** — calls `apply_rename_core(&mut self.node_type_registry, &mut self.active_node_network_name, old, new)`, then handles navigation history, clipboard, validation, dirty/refresh, undo push.
2. **`RenameNetworkCommand::undo/redo`** — calls `apply_rename_core(ctx.node_type_registry, ctx.active_network_name, ...)`. Replaces today's duplicated `do_rename` method.
3. **`rename_namespace` (main method)** — loops calling `apply_rename_core` for each pair, then handles navigation history (all at once), clipboard (all at once), dirty/refresh, undo push.
4. **`RenameNamespaceCommand::undo/redo`** — loops calling `apply_rename_core` for each pair.

This eliminates the existing 75-line duplication between `rename_node_network` and `RenameNetworkCommand::do_rename`, and prevents it from being tripled by `rename_namespace`.

#### Delete: `check_delete_references`

Extract as a private helper in `structure_designer.rs`:

```rust
/// Check if any network outside `targets` references any network in `targets`.
/// Returns Ok(()) if safe to delete, or Err with details if blocked.
///
/// Intra-set references (networks in `targets` referencing each other) are not blocking —
/// they're all being deleted together.
fn check_delete_references(
    registry: &NodeTypeRegistry,
    targets: &HashSet<&str>,
) -> Result<(), String>
```

**Callers (2 total):**

1. **`delete_node_network`** — `check_delete_references(&self.node_type_registry, &HashSet::from([network_name]))`. Replaces the inline reference-checking loop.
2. **`delete_namespace`** — `check_delete_references(&self.node_type_registry, &affected_set)`. Gets the intra-set exclusion for free.

The remaining delete concerns (snapshot, remove, active network, nav history, clipboard) are straightforward loops in `delete_namespace` and not worth sharing — the single-network and namespace variants have slightly different predicates (exact match vs. prefix match for active network, set membership vs. single name for clipboard) that would make a shared helper awkward.

### New API Functions

```rust
// In rust/src/api/structure_designer/structure_designer_api.rs

#[flutter_rust_bridge::frb(sync)]
pub fn rename_namespace(old_prefix: &str, new_prefix: &str) -> bool

#[flutter_rust_bridge::frb(sync)]
pub fn delete_namespace(prefix: &str) -> APIResult
```

### New Methods on StructureDesigner

#### `rename_namespace`

```rust
pub fn rename_namespace(&mut self, old_prefix: &str, new_prefix: &str) -> bool
```

**Logic**:

1. **Collect affected networks**: Find all network names starting with `old_prefix.` (trailing dot — exact prefix match, not substring).
2. **Validate**: For each affected network, compute new name (replace `old_prefix` prefix with `new_prefix`). Check no new name collides with an existing network or builtin type. If any collision → return `false`. If no networks match → return `false`.
3. **Rename all**: For each affected network, call `apply_rename_core()`.
4. **Push a single `RenameNamespaceCommand`** with the list of (old, new) pairs.
5. **Mark dirty + full refresh** once.

#### `delete_namespace`

```rust
pub fn delete_namespace(&mut self, prefix: &str) -> Result<(), String>
```

**Logic**:

1. **Collect affected networks**: Find all network names starting with `prefix.`.
2. **Validate references**: For each affected network, check if any node in any network *outside* the prefix set references it. If so, return error with details. (References between networks *within* the prefix are OK — they're all being deleted.)
3. **Snapshot all**: Capture `SerializableNodeNetwork` snapshots for each affected network (for undo).
4. **Remove all** from registry.
5. **Update active network** if it was under the prefix (set to `None`).
6. **Update navigation history**: Remove all affected network names.
7. **Clear clipboard** if it references any deleted network.
8. **Push a single `DeleteNamespaceCommand`**.
9. **Mark dirty + full refresh** once.

### New Undo Commands

#### `RenameNamespaceCommand`

```rust
// In rust/src/structure_designer/undo/commands/rename_namespace.rs

#[derive(Debug)]
pub struct RenameNamespaceCommand {
    /// List of (old_name, new_name) pairs for all affected networks.
    pub renames: Vec<(String, String)>,
}

impl UndoCommand for RenameNamespaceCommand {
    fn description(&self) -> &str { "Rename namespace" }

    fn undo(&self, ctx: &mut UndoContext) {
        for (old_name, new_name) in &self.renames {
            apply_rename_core(ctx.node_type_registry, ctx.active_network_name, new_name, old_name);  // reverse
        }
    }

    fn redo(&self, ctx: &mut UndoContext) {
        for (old_name, new_name) in &self.renames {
            apply_rename_core(ctx.node_type_registry, ctx.active_network_name, old_name, new_name);  // forward
        }
    }

    fn refresh_mode(&self) -> UndoRefreshMode { UndoRefreshMode::Full }
}
```

#### `DeleteNamespaceCommand`

```rust
// In rust/src/structure_designer/undo/commands/delete_namespace.rs

pub struct DeleteNamespaceCommand {
    /// Snapshots of all deleted networks (for undo restoration).
    pub network_snapshots: Vec<(String, SerializableNodeNetwork)>,
    pub active_network_before: Option<String>,
    pub active_network_after: Option<String>,
}

impl UndoCommand for DeleteNamespaceCommand {
    fn description(&self) -> &str { "Delete namespace" }

    fn undo(&self, ctx: &mut UndoContext) {
        // Restore all deleted networks from snapshots
        for (_name, snapshot) in &self.network_snapshots {
            if let Ok(network) = serializable_to_node_network(
                snapshot,
                &ctx.node_type_registry.built_in_node_types,
                None,
            ) {
                ctx.node_type_registry.add_node_network(network);
            }
        }
        *ctx.active_network_name = self.active_network_before.clone();
    }

    fn redo(&self, ctx: &mut UndoContext) {
        // Re-delete all networks
        for (name, _snapshot) in &self.network_snapshots {
            ctx.node_type_registry.node_networks.remove(name);
        }
        *ctx.active_network_name = self.active_network_after.clone();
    }

    fn refresh_mode(&self) -> UndoRefreshMode { UndoRefreshMode::Full }
}
```

Manual `Debug` impl needed (same pattern as `DeleteNetworkCommand`).

## Flutter Changes

### `node_network_tree_view.dart` — Context Menu + Rename UI

**New state fields** (mirroring list view pattern):

```dart
String? _editingNodeFullName;  // fullName of node being renamed (leaf or namespace)
final TextEditingController _renameController = TextEditingController();
final FocusNode _renameFocusNode = FocusNode();
```

**Context menu** on right-click (both leaf and namespace nodes):

```dart
onSecondaryTap: () {
  showMenu(
    context: context,
    position: position,
    items: [
      const PopupMenuItem(value: 'rename', child: Text('Rename')),
      const PopupMenuItem(value: 'delete', child: Text('Delete')),
    ],
  ).then((value) {
    if (value == 'rename') _startRenaming(node);
    if (value == 'delete') _handleDelete(context, node);
  });
},
```

**Double-click** triggers rename (both leaf and namespace).

**Rename TextField**: When `_editingNodeFullName == node.fullName`, replace the `Text` widget with a compact inline `TextField`.
- Leaf: pre-filled with `getSimpleName(fullName)`
- Namespace: pre-filled with `node.label`
- ESC cancels, Enter/blur commits

**Rename commit logic**:

```dart
void _commitRename() {
  final newSegment = _renameController.text.trim();
  if (newSegment.isEmpty || _editingNodeFullName == null) {
    _cancelRename();
    return;
  }

  final node = _findNodeByFullName(_editingNodeFullName!);
  if (node == null) { _cancelRename(); return; }

  bool success = true;
  if (node.isLeaf) {
    final oldFullName = node.fullName!;
    final namespace = getNamespace(oldFullName);
    final newFullName = combineQualifiedName(namespace, newSegment);
    if (newFullName != oldFullName) {
      success = widget.model.renameNodeNetwork(oldFullName, newFullName);
    }
  } else {
    final oldPrefix = node.fullName!;
    final parentNamespace = getNamespace(oldPrefix);
    final newPrefix = combineQualifiedName(parentNamespace, newSegment);
    if (newPrefix != oldPrefix) {
      success = widget.model.renameNamespace(oldPrefix, newPrefix);
    }
  }

  if (!success) {
    // Show feedback — name collision or other validation failure
    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(content: Text('Rename failed: name already exists')),
    );
  }

  setState(() { _editingNodeFullName = null; });
}
```

**Delete handler**:

```dart
void _handleDelete(BuildContext context, _NodeNetworkTreeNode node) {
  if (node.isLeaf) {
    // Single network delete — same flow as action bar
    _showDeleteConfirmation(context, node.fullName!, [node.fullName!]);
  } else {
    // Namespace delete — collect all leaf descendants
    final affectedNetworks = _collectLeafNames(node);
    _showNamespaceDeleteConfirmation(context, node.fullName!, affectedNetworks);
  }
}
```

**Delete confirmation for namespace** — dialog lists all affected networks:

```dart
Future<void> _showNamespaceDeleteConfirmation(
  BuildContext context,
  String prefix,
  List<String> affectedNetworks,
) {
  showDraggableAlertDialog(
    context: context,
    title: Text('Delete Namespace'),
    content: Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text('Delete "$prefix" and all ${affectedNetworks.length} networks within it?'),
        const SizedBox(height: 8),
        const Text('Networks to be deleted:', style: TextStyle(fontWeight: FontWeight.bold)),
        ...affectedNetworks.map((n) => Text('  • $n')),
      ],
    ),
    actions: [
      TextButton(onPressed: () => Navigator.of(context).pop(), child: Text('Cancel')),
      TextButton(onPressed: () { /* call model.deleteNamespace(prefix) */ }, child: Text('Delete')),
    ],
  );
}
```

**Tree expansion state migration** after namespace rename:

```dart
final toRemove = <String>[];
final toAdd = <String>[];
for (final ns in _expandedNamespaces) {
  if (ns == oldPrefix || ns.startsWith('$oldPrefix.')) {
    toRemove.add(ns);
    toAdd.add(ns.replaceFirst(oldPrefix, newPrefix));
  }
}
_expandedNamespaces.removeAll(toRemove);
_expandedNamespaces.addAll(toAdd);
```

### `structure_designer_model.dart` — New Methods

```dart
bool renameNamespace(String oldPrefix, String newPrefix) {
  final success = structure_designer_api.renameNamespace(
    oldPrefix: oldPrefix,
    newPrefix: newPrefix,
  );
  if (success) {
    refreshFromKernel();
  }
  return success;
}

String? deleteNamespace(String prefix) {
  final result = structure_designer_api.deleteNamespace(prefix: prefix);
  if (result.success) {
    // Match existing deleteNodeNetwork pattern: clear active view if it was
    // under the deleted prefix, then refresh network list manually.
    if (nodeNetworkView != null &&
        nodeNetworkView!.name.startsWith('$prefix.')) {
      nodeNetworkView = null;
    }
    nodeNetworkNames =
        structure_designer_api.getNodeNetworksWithValidation() ?? [];
    notifyListeners();
    return null;
  }
  return result.errorMessage;
}
```

### `node_network_list_view.dart` — Add Delete to Context Menu

While we're adding context menus, also add "Delete" to the list view's existing right-click menu (currently only has "Rename"). Same confirmation dialog pattern.

## Edge Cases

### Naming Conflicts (Rename)

If renaming namespace `A` → `B` would cause any network name collision (e.g., `B.Foo` already exists), the Rust API returns `false` and nothing changes. Flutter shows a SnackBar indicating the rename failed.

### Reference Conflicts (Delete)

If any network under the namespace is referenced by a network *outside* the namespace, the entire delete operation is rejected. The error message lists the blocking references (same pattern as single-network delete).

References *within* the namespace are not blocking — if `Physics.Mechanics.Spring` references `Physics.Mechanics.Damper` and both are being deleted, that's fine.

### Empty Namespace After Rename/Delete

If all networks under a namespace are renamed/deleted away, the namespace node disappears from the tree. This is correct — no cleanup needed.

### Active Network Under Renamed/Deleted Namespace

- **Rename**: Active network name updates to the new prefix. After `refreshFromKernel()`, tree auto-expands ancestors.
- **Delete**: Active network becomes `None`. The node editor clears.

### Dots in User Input (Rename)

If the user types a segment containing dots (e.g., renames leaf `Spring` to `Spring.v2`), the network gains an extra hierarchy level. Consistent with list view behavior.

### Root-Level Namespace

Works the same — `getNamespace(prefix)` returns `""`, `combineQualifiedName("", newSegment)` returns `newSegment`.

### Namespace with Single Child

If a namespace has only one child network (e.g., `Physics.Spring`), renaming or deleting the namespace is equivalent to renaming/deleting that single network. The batch operation handles this naturally.

## Implementation Phases

### Phase 1: Context Menu + Leaf Operations (Flutter only)

- Add context menu (right-click) to tree view nodes with "Rename" and "Delete" options
- Add double-click → Rename on leaf nodes
- Add rename state, TextField, commit/cancel logic for leaf nodes (simple name editing)
- Add leaf delete with confirmation dialog (calls existing `model.deleteNodeNetwork`)
- Also add "Delete" to list view context menu
- **No Rust changes** — uses existing single-network APIs

### Phase 2: Namespace Operations (Rust + Flutter)

- Extract `apply_rename_core()` into `undo/commands/rename_helpers.rs`, refactor existing `rename_node_network` and `RenameNetworkCommand` to use it
- Extract `check_delete_references()` in `structure_designer.rs`, refactor existing `delete_node_network` to use it
- Add `rename_namespace()` method + `RenameNamespaceCommand` undo command
- Add `delete_namespace()` method + `DeleteNamespaceCommand` undo command
- Add API functions for both
- Run FRB codegen
- Add model methods
- Enable rename + delete gestures on namespace nodes in tree view
- Add expansion state migration for namespace rename
- Add namespace delete confirmation dialog (lists affected networks)
- Add tests

### Phase 3: Polish

- SnackBar feedback on failed rename (name collision) or failed delete (reference conflict)
- Ensure rename field auto-selects text on activation
- Test with imported libraries (large network lists)

## Files to Modify

### Phase 1
| File | Change |
|------|--------|
| `lib/structure_designer/node_networks_list/node_network_tree_view.dart` | Context menu, double-click rename, leaf rename UI, leaf delete with confirmation |
| `lib/structure_designer/node_networks_list/node_network_list_view.dart` | Add "Delete" to existing context menu |

### Phase 2
| File | Change |
|------|--------|
| `rust/src/structure_designer/structure_designer.rs` | Extract `check_delete_references()`, refactor `rename_node_network()` to use `apply_rename_core()`, add `rename_namespace()`, add `delete_namespace()` |
| `rust/src/structure_designer/undo/commands/rename_helpers.rs` | New `apply_rename_core()` shared helper |
| `rust/src/structure_designer/undo/commands/rename_network.rs` | Refactor to use `apply_rename_core()` (remove duplicated `do_rename`) |
| `rust/src/structure_designer/undo/commands/rename_namespace.rs` | New `RenameNamespaceCommand` |
| `rust/src/structure_designer/undo/commands/delete_namespace.rs` | New `DeleteNamespaceCommand` |
| `rust/src/structure_designer/undo/commands/mod.rs` | Register `rename_helpers`, `rename_namespace`, `delete_namespace` modules |
| `rust/src/api/structure_designer/structure_designer_api.rs` | Add `rename_namespace()` and `delete_namespace()` API functions |
| `lib/src/rust/` | Regenerated FRB bindings |
| `lib/structure_designer/structure_designer_model.dart` | Add `renameNamespace()` and `deleteNamespace()` methods |
| `lib/structure_designer/node_networks_list/node_network_tree_view.dart` | Enable namespace rename + delete, expansion state migration, namespace delete confirmation dialog |
| `rust/tests/structure_designer/structure_designer_test.rs` | Tests for `rename_namespace`, `delete_namespace` |
| `rust/tests/structure_designer/undo_test.rs` | Tests for `RenameNamespaceCommand`, `DeleteNamespaceCommand` undo/redo |

## Estimated Scope

- **Phase 1**: ~120 lines of Dart (context menu + leaf rename + leaf delete + list view delete)
- **Phase 2**: ~250 lines of Rust (helper extraction + 2 methods + 2 undo commands), ~50 lines of Dart, ~150 lines of tests
- **Phase 3**: ~20 lines of Dart

Total: ~590 lines of new/modified code.
