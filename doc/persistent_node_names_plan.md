# Persistent Node Names Plan

## Overview

Currently, node display names (like `expr1`, `cuboid2`) are generated on-the-fly during serialization and query operations. This causes a bug where the `evaluate` command cannot find nodes by their displayed names, and creates instability where node names can change as the network evolves.

This plan introduces **persistent node names** - every node gets a unique name assigned at creation time that never changes.

## Problem Statement

### Current Behavior

1. **Node creation**: `NodeNetwork::add_node()` sets `custom_name: None`
2. **Query output**: `NetworkSerializer::generate_names()` generates names like `expr1` on-the-fly
3. **Evaluate lookup**: `find_node_id_by_name()` searches `custom_name` field, which is `None`
4. **Result**: `atomcad-cli evaluate expr1` fails with "Node not found"

### Additional Issues

- Names can change after network modifications (delete a node, counters shift)
- Three separate implementations of the same name generation algorithm
- ~150 lines of duplicated code across `NetworkSerializer`, `NetworkEditor`, and `StructureDesigner`

## Solution

Assign a unique, persistent name to every node at creation time. The name is stored in `custom_name` and never changes.

### Benefits

1. **Stability**: Node names are permanent - agents can rely on them
2. **Simplicity**: No complex name generation at query time
3. **Less code**: Delete ~150 lines of duplicated logic
4. **Consistency**: Same name shown in query, usable in evaluate

## Implementation Plan

### Phase 1: Add Name Generation to NodeNetwork

**Goal**: Add the core name generation helper method.

**File**: `rust/src/structure_designer/node_network.rs`

**Changes**:

1. Add helper method to generate unique names:

```rust
/// Generate a unique display name for a new node of the given type.
///
/// Scans existing nodes to find the highest counter used for this type,
/// then returns `{type}{max+1}`. Names are never reused even if nodes
/// are deleted, ensuring stability for external references.
pub fn generate_unique_display_name(&self, node_type: &str) -> String {
    let mut max_counter = 0;
    for node in self.nodes.values() {
        if let Some(ref name) = node.custom_name {
            if let Some(num_str) = name.strip_prefix(node_type) {
                if let Ok(num) = num_str.parse::<u32>() {
                    max_counter = max_counter.max(num);
                }
            }
        }
    }
    format!("{}{}", node_type, max_counter + 1)
}
```

2. Update `add_node()` to assign a name:

```rust
pub fn add_node(&mut self, node_type_name: &str, ...) -> u64 {
    let node_id = self.next_node_id;
    let display_name = self.generate_unique_display_name(node_type_name);

    let node = Node {
        id: node_id,
        node_type_name: node_type_name.to_string(),
        custom_name: Some(display_name),  // Changed from None
        // ... rest unchanged
    };
    // ...
}
```

3. Update `duplicate_node()` similarly (around line 819):

```rust
let display_name = self.generate_unique_display_name(&original_node.node_type_name);

let duplicated_node = Node {
    id: new_node_id,
    node_type_name: original_node.node_type_name.clone(),
    custom_name: Some(display_name),  // Changed from None
    // ...
};
```

**Tests for Phase 1** (add to `rust/tests/structure_designer/node_network_test.rs` or similar):

```rust
#[test]
fn test_generate_unique_display_name_empty_network() {
    let network = create_empty_network();
    assert_eq!(network.generate_unique_display_name("cuboid"), "cuboid1");
}

#[test]
fn test_generate_unique_display_name_increments() {
    let mut network = create_empty_network();
    network.add_node("cuboid", DVec2::ZERO);  // Gets cuboid1
    network.add_node("cuboid", DVec2::ZERO);  // Gets cuboid2
    assert_eq!(network.generate_unique_display_name("cuboid"), "cuboid3");
}

#[test]
fn test_generate_unique_display_name_after_deletion() {
    let mut network = create_empty_network();
    let id1 = network.add_node("cuboid", DVec2::ZERO);  // cuboid1
    let _id2 = network.add_node("cuboid", DVec2::ZERO);  // cuboid2
    network.delete_node(id1);  // Delete cuboid1
    // Next cuboid should be cuboid3, NOT cuboid1 (no reuse)
    assert_eq!(network.generate_unique_display_name("cuboid"), "cuboid3");
}

#[test]
fn test_add_node_assigns_custom_name() {
    let mut network = create_empty_network();
    let id = network.add_node("sphere", DVec2::ZERO);
    let node = network.nodes.get(&id).unwrap();
    assert_eq!(node.custom_name, Some("sphere1".to_string()));
}

#[test]
fn test_duplicate_node_gets_unique_name() {
    let mut network = create_empty_network();
    let id1 = network.add_node("cuboid", DVec2::ZERO);  // cuboid1
    let id2 = network.duplicate_node(id1).unwrap();
    let node2 = network.nodes.get(&id2).unwrap();
    assert_eq!(node2.custom_name, Some("cuboid2".to_string()));
}
```

**Run after Phase 1**: `cargo test node_network`

---

### Phase 2: Migration for Existing Files

**Goal**: Ensure old `.cnnd` files get names assigned when loaded.

**File**: `rust/src/structure_designer/serialization/node_networks_serialization.rs`

**Changes**:

Update `serializable_to_node_network()` to assign names after loading:

```rust
pub fn serializable_to_node_network(...) -> io::Result<NodeNetwork> {
    // ... existing code to create network and load nodes ...

    // Migration: assign names to nodes without custom_name (old files)
    let nodes_needing_names: Vec<u64> = network.nodes.iter()
        .filter(|(_, node)| node.custom_name.is_none())
        .map(|(id, _)| *id)
        .collect();

    for node_id in nodes_needing_names {
        if let Some(node) = network.nodes.get(&node_id) {
            let node_type = node.node_type_name.clone();
            let name = network.generate_unique_display_name(&node_type);
            if let Some(node) = network.nodes.get_mut(&node_id) {
                node.custom_name = Some(name);
            }
        }
    }

    Ok(network)
}
```

**Note**: The order of name assignment for migrated nodes doesn't need to match the old serializer's topological order - those names were ephemeral anyway and no external system should depend on them.

**Tests for Phase 2** (add to `rust/tests/structure_designer/serialization_test.rs` or similar):

```rust
#[test]
fn test_migration_assigns_names_to_old_nodes() {
    // Create a serializable network with nodes that have no custom_name
    let serializable = SerializableNodeNetwork {
        nodes: vec![
            SerializableNode { id: 1, node_type_name: "int".into(), custom_name: None, ... },
            SerializableNode { id: 2, node_type_name: "int".into(), custom_name: None, ... },
        ],
        ...
    };

    let network = serializable_to_node_network(&serializable, &built_ins, None).unwrap();

    // Both nodes should now have names
    assert!(network.nodes.get(&1).unwrap().custom_name.is_some());
    assert!(network.nodes.get(&2).unwrap().custom_name.is_some());

    // Names should be unique
    let name1 = network.nodes.get(&1).unwrap().custom_name.as_ref().unwrap();
    let name2 = network.nodes.get(&2).unwrap().custom_name.as_ref().unwrap();
    assert_ne!(name1, name2);
}

#[test]
fn test_migration_preserves_existing_custom_names() {
    let serializable = SerializableNodeNetwork {
        nodes: vec![
            SerializableNode { id: 1, node_type_name: "int".into(), custom_name: Some("myint".into()), ... },
            SerializableNode { id: 2, node_type_name: "int".into(), custom_name: None, ... },
        ],
        ...
    };

    let network = serializable_to_node_network(&serializable, &built_ins, None).unwrap();

    // Existing name preserved
    assert_eq!(network.nodes.get(&1).unwrap().custom_name, Some("myint".to_string()));
    // New name assigned to the other
    assert!(network.nodes.get(&2).unwrap().custom_name.is_some());
}
```

**Run after Phase 2**: `cargo test serialization` and `cargo test cnnd_roundtrip`

---

### Phase 3: Simplify find_node_id_by_name

**Goal**: Remove the duplicated name generation algorithm from the lookup function.

**File**: `rust/src/structure_designer/structure_designer.rs`

**Changes**:

Replace the current ~97-line implementation with a simple search:

```rust
/// Find a node ID by its display name in the active network.
///
/// Since all nodes have persistent names assigned at creation,
/// this is a simple search through the custom_name fields.
pub fn find_node_id_by_name(&self, name: &str) -> Option<u64> {
    let network_name = self.active_node_network_name.as_ref()?;
    let network = self.node_type_registry.node_networks.get(network_name)?;

    for (node_id, node) in &network.nodes {
        if node.custom_name.as_deref() == Some(name) {
            return Some(*node_id);
        }
    }

    None
}
```

**Delete**: The `topological_sort_nodes()` and `dfs_visit_for_sort()` helper methods (~45 lines).

**Tests for Phase 3**: The existing `find_node_id_by_name` tests should continue to pass, but now with simpler implementation. Add:

```rust
#[test]
fn test_find_node_by_name_ui_created() {
    // Simulate UI-created node (goes through add_node)
    let mut designer = create_test_designer();
    designer.add_node("expr", DVec2::ZERO);  // Gets "expr1"

    let found = designer.find_node_id_by_name("expr1");
    assert!(found.is_some());
}

#[test]
fn test_find_node_by_name_text_created() {
    let mut designer = create_test_designer();
    designer.edit_network("mynode = int { value: 42 }").unwrap();

    let found = designer.find_node_id_by_name("mynode");
    assert!(found.is_some());
}
```

**Run after Phase 3**: `cargo test find_node` and manual CLI test:
```bash
# With atomCAD running, create nodes via UI, then:
atomcad-cli query      # Note the names shown
atomcad-cli evaluate <name>  # Should work now
```

---

### Phase 4: Simplify NetworkSerializer

**Goal**: Remove name generation logic since all nodes now have names.

**File**: `rust/src/structure_designer/text_format/network_serializer.rs`

**Changes**:

1. Remove `generate_names()` method (~50 lines)
2. Remove `type_counters` field
3. Update `serialize()` to use `node.custom_name` directly:

```rust
fn get_node_name(&self, node_id: u64) -> Option<&str> {
    self.network.nodes.get(&node_id)
        .and_then(|node| node.custom_name.as_deref())
}
```

The topological sort is still needed for output ordering, but name generation is not.

**Tests for Phase 4**: Existing serialization snapshot tests should pass. Verify query output:

```rust
#[test]
fn test_serialize_uses_persistent_names() {
    let mut network = create_network_with_nodes();
    // Manually set custom_name to verify serializer uses it
    network.nodes.get_mut(&1).unwrap().custom_name = Some("mybox".to_string());

    let output = serialize_network(&network);
    assert!(output.contains("mybox = cuboid"));
}
```

**Run after Phase 4**: `cargo test node_snapshots` and `cargo test serialize`

---

### Phase 5: Simplify NetworkEditor

**Goal**: Remove duplicated name generation from the editor.

**File**: `rust/src/structure_designer/text_format/network_editor.rs`

**Changes**:

1. Simplify `build_existing_name_map()` (~60 lines → ~15 lines):

```rust
fn build_existing_name_map(&mut self) {
    self.name_to_id.clear();
    self.id_to_name.clear();

    for (node_id, node) in &self.network.nodes {
        if let Some(ref name) = node.custom_name {
            self.name_to_id.insert(name.clone(), *node_id);
            self.id_to_name.insert(*node_id, name.clone());
        }
    }
}
```

2. Remove `topological_sort_existing()` and `dfs_visit_existing()` methods (~40 lines) - no longer needed for name generation.

**Tests for Phase 5**: Edit/query roundtrip tests should pass:

```rust
#[test]
fn test_editor_builds_name_map_from_custom_names() {
    let mut designer = create_test_designer();
    // Create nodes via UI (assigns persistent names)
    designer.add_node("int", DVec2::ZERO);  // int1
    designer.add_node("int", DVec2::ZERO);  // int2

    // Edit should be able to reference these names
    let result = designer.edit_network("result = expr { x: int1, y: int2, expression: \"x + y\" }");
    assert!(result.is_ok());
}
```

**Run after Phase 5**: `cargo test text_format` and `cargo test edit`

---

### Phase 6: Update NetworkEditor Node Creation

**Goal**: Ensure text-format created nodes respect existing names.

**File**: `rust/src/structure_designer/text_format/network_editor.rs`

**Current behavior** (line 379): When parsing `mybox = cuboid {...}`, it sets `node.custom_name = Some("mybox")`.

**No change needed**: This already works correctly. The user-specified name takes precedence.

**Edge case**: If user specifies a name that conflicts with an existing node, the current behavior keeps both (the editor tracks its own `name_to_id` map). This is acceptable - the file will have two nodes with the same logical name, which may cause confusion but isn't a crash.

**Tests for Phase 6** (verification only):

```rust
#[test]
fn test_text_format_custom_name_preserved() {
    let mut designer = create_test_designer();
    designer.edit_network("mybox = cuboid { width: 1, height: 2, depth: 3 }").unwrap();

    let node_id = designer.find_node_id_by_name("mybox").unwrap();
    let network = designer.get_active_network().unwrap();
    let node = network.nodes.get(&node_id).unwrap();

    assert_eq!(node.custom_name, Some("mybox".to_string()));
}

#[test]
fn test_text_format_auto_name_when_not_specified() {
    let mut designer = create_test_designer();
    // Note: current text format requires names, but if we ever support anonymous nodes:
    // designer.edit_network("cuboid { width: 1 }").unwrap();
    // The node should get an auto-generated name like "cuboid1"
}
```

**Run after Phase 6**: Full test suite: `cargo test`

---

## File Summary

| File | Changes | Lines |
|------|---------|-------|
| `node_network.rs` | Add `generate_unique_display_name()`, update `add_node()`, `duplicate_node()` | +20 |
| `node_networks_serialization.rs` | Add migration logic in `serializable_to_node_network()` | +15 |
| `structure_designer.rs` | Simplify `find_node_id_by_name()`, delete helpers | -85 |
| `network_serializer.rs` | Remove `generate_names()`, simplify name lookup | -45 |
| `network_editor.rs` | Simplify `build_existing_name_map()`, delete sort helpers | -55 |
| **Net change** | | **-150** |

---

## Testing Summary

Tests are distributed across phases (see each phase for specific tests). After all phases complete, run the full verification:

### Final Test Commands

```bash
# All Rust tests
cd rust && cargo test

# Specific test categories
cargo test node_network      # Phase 1 tests
cargo test serialization     # Phase 2 tests
cargo test cnnd_roundtrip    # File format roundtrip
cargo test find_node         # Phase 3 tests
cargo test node_snapshots    # Phase 4 tests (may need `cargo insta review`)
cargo test text_format       # Phase 5 tests
```

### Manual Integration Tests

1. **CLI round-trip** (after Phase 3):
   ```bash
   # With atomCAD running:
   atomcad-cli edit --code="a = int { value: 1 }"
   atomcad-cli query           # Shows "a = int {...}"
   atomcad-cli evaluate a      # Should work
   ```

2. **UI-created nodes** (after Phase 3):
   - Create nodes via UI (drag from palette)
   - Run `atomcad-cli query` - shows names like `int1`, `expr2`
   - Run `atomcad-cli evaluate int1` - should work

3. **File persistence** (after Phase 2):
   - Create nodes via UI, save file
   - Close and reopen atomCAD
   - Run `atomcad-cli query` - same names as before

---

## Migration Notes

### Backwards Compatibility

- Old files load correctly (names assigned on load)
- New files include `custom_name` for all nodes
- No breaking changes to file format (field was already optional)

### Agent Impact

- Positive: Names are now stable and reliable
- Agents should be aware that deleting and recreating a node gives a new name (e.g., `expr3` not `expr1`)

---

## Dependencies Between Phases

```
Phase 1 (NodeNetwork changes)
    │
    ├──► Phase 2 (Migration) - can run in parallel
    │
    └──► Phase 3 (Simplify find_node_id_by_name)
              │
              ├──► Phase 4 (Simplify NetworkSerializer)
              │
              └──► Phase 5 (Simplify NetworkEditor)
                        │
                        └──► Phase 6 (Verify NetworkEditor creation)
```

Phases 1 and 2 can be done together. Phases 3-5 depend on Phase 1. Phase 6 is verification only.
