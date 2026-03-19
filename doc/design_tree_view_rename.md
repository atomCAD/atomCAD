# Design: Rename Node Networks in Tree View

## Problem

Users can only rename node networks in the flat list view, not the tree (hierarchy) view. This forces users to:

1. Switch to the list view
2. Scroll to find the network (especially painful with imported libraries)
3. Rename it
4. Switch back to tree view

This is unintuitive — the rename gesture (double-click, right-click → Rename) should work identically in both views.

Additionally, the tree view exposes **namespace nodes** (virtual folder groupings derived from dot-delimited names like `Physics.Mechanics.Spring`). Users should be able to rename a namespace segment (e.g., `Mechanics` → `Dynamics`), which batch-renames all networks sharing that prefix.

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

- **`namespace_utils.dart`** — `getSegments()`, `getSimpleName()`, `getNamespace()`, `combineQualifiedName()` for parsing qualified names.
- **`node_network_tree_view.dart`** — Builds a tree from flat names using `_buildTreeFromNames()`. Each `_NodeNetworkTreeNode` has `label` (segment), `fullName` (qualified path), `isLeaf`, `children`. Currently supports click-to-activate (leaf) and click-to-expand (namespace), but **no rename UI**.
- **`node_network_list_view.dart`** — Flat list with rename support: `_editingNetworkName` state, `TextEditingController`, double-click and context menu triggers, ESC to cancel, Enter/blur to commit. Calls `model.renameNodeNetwork(oldName, newName)`.
- **`structure_designer_model.dart`** — `renameNodeNetwork(oldName, newName)` calls the Rust API, then `refreshFromKernel()` on success.

### Rust Side

- **`structure_designer.rs:724`** — `rename_node_network(old_name, new_name) -> bool`: Validates (old exists, new doesn't exist, not a builtin name), renames the single network, updates active network name, navigation history, all node references across all networks, clipboard, and backtick references in comments/metadata. Pushes `RenameNetworkCommand` to undo stack.
- **`undo/commands/rename_network.rs`** — `RenameNetworkCommand { old_name, new_name }` with symmetric `do_rename()` for undo/redo. Single-network operation.
- **No batch/prefix rename** exists on the Rust side.

### API Surface

Single function: `rename_node_network(old_name: &str, new_name: &str) -> bool` (sync FFI).

## Design

### Two Rename Scenarios

#### 1. Leaf Node Rename (simple — no Rust changes needed)

User double-clicks or right-clicks a **leaf node** (actual network) in the tree view.

- The text field shows **only the simple name** (last segment), e.g., `Spring` not `Physics.Mechanics.Spring`.
- On commit, Flutter reconstructs the full qualified name: `combineQualifiedName(getNamespace(oldFullName), newSimpleName)`.
- Calls the existing `model.renameNodeNetwork(oldFullName, newFullName)`.

This is purely a Flutter UI change — the Rust API already handles the single rename with all cascading updates.

**Edge case**: If the user types a name containing dots (e.g., `Spring.v2`), the network moves into a new sub-namespace. This is acceptable — it's how the list view already works (names with dots create hierarchy).

#### 2. Namespace Node Rename (requires new Rust API)

User double-clicks or right-clicks a **namespace node** (virtual folder) in the tree view.

- The text field shows **only the namespace segment label**, e.g., `Mechanics` not `Physics.Mechanics`.
- On commit, Flutter computes the old and new prefix:
  - Old prefix: `Physics.Mechanics` (the `fullName` of the namespace node)
  - New prefix: `Physics.Dynamics` (replace last segment with user input)
- Calls a **new** API: `model.renameNamespace(oldPrefix, newPrefix)`.
- This renames ALL networks whose name starts with `oldPrefix.` — in one atomic operation, one undo step.

**Example**: Renaming `Mechanics` → `Dynamics` in the tree above:
- `Physics.Mechanics.Spring` → `Physics.Dynamics.Spring`
- `Physics.Mechanics.Damper` → `Physics.Dynamics.Damper`

### Rust Changes

#### New API Function

```rust
// In rust/src/api/structure_designer/structure_designer_api.rs

#[flutter_rust_bridge::frb(sync)]
pub fn rename_namespace(old_prefix: &str, new_prefix: &str) -> bool
```

#### New Method on StructureDesigner

```rust
// In rust/src/structure_designer/structure_designer.rs

pub fn rename_namespace(&mut self, old_prefix: &str, new_prefix: &str) -> bool
```

**Logic**:

1. **Collect affected networks**: Find all network names starting with `old_prefix.` (note the trailing dot — exact prefix match, not substring).
2. **Validate**: For each affected network, compute new name (replace `old_prefix` prefix with `new_prefix`). Check no new name collides with an existing network or builtin type. If any collision → return `false`.
3. **Rename all**: For each affected network, perform the same rename steps as `rename_node_network` (remove from registry, update internal name, re-insert, update node references, clipboard, backtick refs). The cascading updates (node type references, etc.) apply to each rename.
4. **Update active network name** if it was under the old prefix.
5. **Update navigation history** for all affected names.
6. **Push a single `RenameNamespaceCommand`** to the undo stack.
7. **Mark dirty + full refresh** (once, not per-network).

**Implementation note**: Since `rename_namespace` is effectively N sequential `rename_node_network` operations minus the per-operation undo/refresh overhead, the core rename logic (steps in `rename_node_network` lines 741–818) should be extracted into a shared helper:

```rust
/// Core rename logic without undo/dirty/refresh side effects.
/// Used by both rename_node_network and rename_namespace.
fn apply_single_rename(
    node_type_registry: &mut NodeTypeRegistry,
    active_name: &mut Option<String>,
    navigation_history: &mut NavigationHistory,
    clipboard: &mut Option<Clipboard>,
    old_name: &str,
    new_name: &str,
)
```

Then `rename_node_network` calls this helper + pushes `RenameNetworkCommand` + dirty/refresh, and `rename_namespace` calls it in a loop + pushes `RenameNamespaceCommand` + dirty/refresh once.

#### New Undo Command

```rust
// In rust/src/structure_designer/undo/commands/rename_namespace.rs

#[derive(Debug)]
pub struct RenameNamespaceCommand {
    /// List of (old_name, new_name) pairs for all affected networks.
    pub renames: Vec<(String, String)>,
}

impl UndoCommand for RenameNamespaceCommand {
    fn description(&self) -> &str {
        "Rename namespace"
    }

    fn undo(&self, ctx: &mut UndoContext) {
        // Apply all renames in reverse: (new_name → old_name)
        for (old_name, new_name) in &self.renames {
            Self::do_single_rename(new_name, old_name, ctx);
        }
    }

    fn redo(&self, ctx: &mut UndoContext) {
        // Apply all renames forward: (old_name → new_name)
        for (old_name, new_name) in &self.renames {
            Self::do_single_rename(old_name, new_name, ctx);
        }
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Full
    }
}
```

The `do_single_rename` method reuses the same logic as `RenameNetworkCommand::do_rename` — either by extracting a shared free function or by calling it directly. The key point: all renames in the `Vec` are applied/reversed as one atomic undo step.

### Flutter Changes

#### `node_network_tree_view.dart` — Add Rename UI

Add rename state management (mirroring the list view pattern):

```dart
// New state fields
String? _editingNodeFullName;  // fullName of node being renamed (leaf or namespace)
final TextEditingController _renameController = TextEditingController();
final FocusNode _renameFocusNode = FocusNode();
```

**Trigger**: Double-tap or right-click context menu on any tree node (leaf or namespace).

**TextField content**:
- Leaf node: show `getSimpleName(fullName)` (just the leaf segment)
- Namespace node: show `node.label` (just the namespace segment)

**Commit logic**:
```dart
void _commitRename() {
  final newSegment = _renameController.text.trim();
  if (newSegment.isEmpty || _editingNodeFullName == null) {
    _cancelRename();
    return;
  }

  final node = _findNodeByFullName(_editingNodeFullName!);
  if (node == null) { _cancelRename(); return; }

  if (node.isLeaf) {
    // Leaf rename: reconstruct full name
    final oldFullName = node.fullName!;
    final namespace = getNamespace(oldFullName);
    final newFullName = combineQualifiedName(namespace, newSegment);
    if (newFullName != oldFullName) {
      widget.model.renameNodeNetwork(oldFullName, newFullName);
    }
  } else {
    // Namespace rename: compute old and new prefix
    final oldPrefix = node.fullName!;  // e.g., "Physics.Mechanics"
    final parentNamespace = getNamespace(oldPrefix);  // e.g., "Physics"
    final newPrefix = combineQualifiedName(parentNamespace, newSegment);
    if (newPrefix != oldPrefix) {
      widget.model.renameNamespace(oldPrefix, newPrefix);
    }
  }

  setState(() { _editingNodeFullName = null; });
}
```

**UI change in `nodeBuilder`**: When `_editingNodeFullName == node.fullName`, replace the `Text` widget with a `TextField` (same styling pattern as list view). The field should be compact and inline within the tree row.

**Keyboard**: ESC cancels (no rename). Enter or blur commits.

#### `structure_designer_model.dart` — Add `renameNamespace`

```dart
void renameNamespace(String oldPrefix, String newPrefix) {
  final success = structure_designer_api.renameNamespace(
    oldPrefix: oldPrefix,
    newPrefix: newPrefix,
  );
  if (success) {
    refreshFromKernel();
  }
}
```

#### Tree Expansion State After Rename

When a namespace is renamed, the expansion state (`_expandedNamespaces`) must be updated:
- Remove the old prefix from `_expandedNamespaces`
- Add the new prefix
- Also update any descendant namespace paths that start with the old prefix

This happens naturally in `_restoreExpansionState()` since it cleans up invalid namespaces — but the new prefix won't be expanded unless we explicitly migrate it. Add a small migration step in `_commitRename()` for namespace renames:

```dart
// Migrate expansion state for renamed namespace
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

## Edge Cases

### Naming Conflicts

If renaming namespace `A` → `B` would cause any network name collision (e.g., `B.Foo` already exists), the Rust API returns `false` and nothing changes. Flutter should show feedback (e.g., a brief SnackBar) indicating the rename failed.

### Empty Namespace After Rename

If all networks under a namespace are renamed away, the namespace node disappears from the tree (it was virtual). This is correct behavior — no cleanup needed.

### Active Network Under Renamed Namespace

If the active network is `Physics.Mechanics.Spring` and the user renames `Mechanics` → `Dynamics`, the active network name must update to `Physics.Dynamics.Spring`. The Rust side handles this in the rename logic. After `refreshFromKernel()`, the Flutter tree auto-expands ancestors of the new active network path.

### Dots in User Input

If the user types a segment containing dots (e.g., renames leaf `Spring` to `Spring.v2`), the network gains an extra hierarchy level. This is consistent with how the list view works — dots in names are how users create hierarchy. No special handling needed.

### Root-Level Namespace

A root-level namespace (no parent) works the same — `getNamespace(oldPrefix)` returns `""`, and `combineQualifiedName("", newSegment)` returns just `newSegment`.

## Implementation Phases

### Phase 1: Leaf Rename in Tree View (Flutter only)

- Add rename state, double-click/context-menu triggers, TextField, commit/cancel to `node_network_tree_view.dart`
- Only leaf nodes are renamable
- Uses existing `model.renameNodeNetwork()`
- **No Rust changes**

### Phase 2: Namespace Rename (Rust + Flutter)

- Extract shared rename helper in `structure_designer.rs`
- Add `rename_namespace()` to `StructureDesigner`
- Add `RenameNamespaceCommand` undo command
- Add `rename_namespace()` API function
- Run FRB codegen
- Add `renameNamespace()` to model
- Enable rename gesture on namespace nodes in tree view
- Add expansion state migration
- Add tests

### Phase 3: Polish

- SnackBar feedback on failed rename (name collision, builtin conflict)
- Ensure rename field auto-selects text on activation
- Test with imported libraries (large network lists)

## Files to Modify

### Phase 1
| File | Change |
|------|--------|
| `lib/structure_designer/node_networks_list/node_network_tree_view.dart` | Add rename state, gestures, TextField, commit/cancel logic |

### Phase 2
| File | Change |
|------|--------|
| `rust/src/structure_designer/structure_designer.rs` | Extract `apply_single_rename()` helper, add `rename_namespace()` method |
| `rust/src/structure_designer/undo/commands/rename_namespace.rs` | New `RenameNamespaceCommand` |
| `rust/src/structure_designer/undo/commands/mod.rs` | Register new command module |
| `rust/src/api/structure_designer/structure_designer_api.rs` | Add `rename_namespace()` API function |
| `lib/src/rust/` | Regenerated FRB bindings |
| `lib/structure_designer/structure_designer_model.dart` | Add `renameNamespace()` method |
| `lib/structure_designer/node_networks_list/node_network_tree_view.dart` | Enable namespace rename, expansion state migration |
| `rust/tests/structure_designer/structure_designer_test.rs` | Tests for `rename_namespace` |
| `rust/tests/structure_designer/undo_test.rs` | Tests for `RenameNamespaceCommand` undo/redo |

## Estimated Scope

- **Phase 1**: ~80 lines of Dart (mostly mirroring existing list view code)
- **Phase 2**: ~150 lines of Rust (helper extraction + new method + undo command), ~30 lines of Dart, ~100 lines of tests
- **Phase 3**: ~20 lines of Dart

Total: ~380 lines of new/modified code. The refactoring of existing rename logic into a shared helper actually *reduces* duplication between `rename_node_network` and its undo command.
