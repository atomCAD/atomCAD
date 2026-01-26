# Node Network Management for atomcad-cli

## Overview

Extend `atomcad-cli` to support node network management operations: listing, creating, deleting, activating, and renaming node networks. This enables AI agents to work with multi-network designs programmatically.

## Current State

### Existing Infrastructure

**AI Assistant Rust API** (`rust/src/api/structure_designer/ai_assistant_api.rs`):
- `ai_query_network()` - serialize active network to text format
- `ai_edit_network()` - parse text commands and apply changes
- `ai_list_networks()` → `Vec<String>` - just names, no validation info
- `ai_get_active_network_info()` → `Option<(name, node_count, has_output)>`
- `ai_list_node_types()` - list available node types
- `ai_describe_node_type()` - describe a node type

**Structure Designer Rust API** (`rust/src/api/structure_designer/structure_designer_api.rs`):
- `get_node_networks_with_validation()` → `Vec<APINetworkWithValidationErrors>` (line 526)
- `add_new_node_network()` (line 538)
- `set_active_node_network(name)` (line 547)
- `rename_node_network(old, new)` → bool (line 617)
- `delete_node_network(name)` → `APIResult` (line 631)

**HTTP Server** (`lib/ai_assistant/http_server.dart`):
- Uses `ai_api.*` functions for AI-facing endpoints (`/query`, `/edit`, `/nodes`, etc.)
- Also imports `sd_api` for some operations

**Current CLI** (`bin/atomcad_cli.dart`):
- Commands: `query`, `edit`, `nodes`, `describe`, `evaluate`, `camera`, `screenshot`, `display`
- Communicates via HTTP to atomCAD server on port 19847
- `query` and `edit` operate on the "active" network

---

## Proposed Commands

### 1. `networks` - List all node networks

```bash
atomcad-cli networks
```

**Output:**
```
Node Networks:
  * Main              (active)
    cube
    pattern           [ERROR: Invalid parameter type]

3 networks (1 with errors)
```

### 2. `networks add` - Create a new node network

```bash
atomcad-cli networks add                    # Auto-named (UNTITLED, UNTITLED1, ...)
atomcad-cli networks add --name "my_net"    # Specific name
```

**Output:**
```
Created network 'my_net' (now active)
```

**Behavior:**
- Auto-activates the new network (matches UI behavior)
- Returns error if name already exists

### 3. `networks delete` - Delete a node network

```bash
atomcad-cli networks delete <name>
```

**Output (success):**
```
Deleted network 'old_cube'
```

**Output (error):**
```
Error: Cannot delete 'cube': referenced by networks: Main, pattern
```

**Behavior:**
- Fails if network is referenced by nodes in other networks
- If deleting active network, clears active (no network selected)

### 4. `networks activate` - Switch active network

```bash
atomcad-cli networks activate <name>
```

**Output:**
```
Switched to network 'cube'
```

**Behavior:**
- Equivalent to clicking a network in the UI
- `query` and `edit` commands now operate on this network

### 5. `networks rename` - Rename a node network

```bash
atomcad-cli networks rename <old-name> <new-name>
```

**Output:**
```
Renamed 'cube' to 'unit_cube'
```

**Behavior:**
- Updates all references in other networks automatically
- Fails if new name already exists

---

## REPL Mode Commands

```
networks                   List all node networks
networks add [--name X]    Create new network
networks delete <name>     Delete a network
networks activate <name>   Switch to a network
networks rename <old> <new>
                           Rename a network
```

---

## Implementation Plan

### Phase 1: Rust API

Add one new function to `rust/src/api/structure_designer/structure_designer_api.rs`, then regenerate FFI bindings.

**Existing functions to use (no changes needed):**
- `get_node_networks_with_validation()` → `Vec<APINetworkWithValidationErrors>`
- `add_new_node_network()` - auto-names and activates
- `set_active_node_network(name)` - activates a network
- `rename_node_network(old, new)` → bool
- `delete_node_network(name)` → `APIResult`

**New function:**

```rust
/// Add a node network with a specific name.
/// Returns success/error. Auto-activates the new network.
#[flutter_rust_bridge::frb(sync)]
pub fn add_node_network_with_name(name: String) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                // Check if name already exists
                if instance.structure_designer.node_type_registry
                    .node_networks.contains_key(&name) {
                    return APIResult {
                        success: false,
                        error_message: format!("Network '{}' already exists", name),
                    };
                }
                instance.structure_designer.add_node_network(&name);
                instance.structure_designer.set_active_node_network_name(Some(name));
                instance.structure_designer.set_dirty(true);
                refresh_structure_designer_auto(instance);
                APIResult { success: true, error_message: String::new() }
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            }
        )
    }
}
```

Then run: `flutter_rust_bridge_codegen generate`

**Tests** (add to `rust/tests/`):
- Test `add_node_network_with_name()` with valid/duplicate names
- Test network operations preserve references correctly

### Phase 2: Dart (HTTP Server + CLI + REPL)

**HTTP endpoints** in `lib/ai_assistant/http_server.dart`:

| Endpoint | Method | Parameters |
|----------|--------|------------|
| `/networks` | GET | none |
| `/networks/add` | POST | `name` (optional) |
| `/networks/delete` | POST | `name` (required) |
| `/networks/activate` | POST | `name` (required) |
| `/networks/rename` | POST | `old`, `new` (required) |

**CLI commands** in `bin/atomcad_cli.dart`:
- Add `networks` command with subcommands: `add`, `delete`, `activate`, `rename`
- Add REPL support for the same commands

**Manual testing:**
```bash
# Start atomCAD
flutter run

# In another terminal
atomcad-cli networks                              # Should show "Main"
atomcad-cli networks add --name "test_net"        # Create new
atomcad-cli networks                              # Should show both, test_net active
atomcad-cli edit --code="s = sphere { radius: 5 }"
atomcad-cli networks activate "Main"              # Switch back
atomcad-cli query                                 # Should show Main's content
atomcad-cli networks delete "test_net"            # Delete
atomcad-cli networks                              # Should show only Main
```

### Phase 3: Documentation

Update `.claude/skills/atomcad/skill.md`:

```markdown
### Node Network Management

atomCAD designs can contain multiple node networks. Use these commands to manage them:

```bash
# List all node networks
atomcad-cli networks

# Create a new network
atomcad-cli networks add                    # Auto-named
atomcad-cli networks add --name "my_shape"  # Specific name

# Switch active network (query/edit will operate on this)
atomcad-cli networks activate <name>

# Delete a network
atomcad-cli networks delete <name>

# Rename a network
atomcad-cli networks rename <old> <new>
```

**Note:** `query` and `edit` always operate on the active network.
```

---

## AI Agent Workflow Example

```bash
# Create a custom node (subnetwork) for a parameterized shape
atomcad-cli networks add --name "rounded_cube"
atomcad-cli edit --replace <<'EOF'
size = parameter { name: "size", type: Int, default: 10 }
radius = parameter { name: "corner_radius", type: Int, default: 2 }
base = cuboid { extent: (size, size, size) }
corner = sphere { radius: radius }
result = intersect { shapes: [base, corner] }
output result
EOF

# Switch to Main and use the custom node
atomcad-cli networks activate "Main"
atomcad-cli edit --code="part = rounded_cube { size: 20, corner_radius: 3, visible: true }"

# Fill with atoms
atomcad-cli edit --code="atoms = atom_fill { shape: part, passivate: true, visible: true }"
```

---

## File Changes Summary

| File | Changes |
|------|---------|
| `rust/src/api/structure_designer/structure_designer_api.rs` | Add `add_node_network_with_name()` |
| `lib/ai_assistant/http_server.dart` | Add `/networks/*` endpoints |
| `bin/atomcad_cli.dart` | Add `networks` command with subcommands + REPL support |
| `.claude/skills/atomcad/skill.md` | Document new commands |

---

## Resolved Questions

1. **`networks delete` behavior:** No `--force` flag needed. In CLI context, typing the delete command is explicit intent. The only constraint is that deletion fails if the network is referenced by other networks (returns error with list of referencing networks). Deleting the active network is allowed - it just clears the active selection.

2. **`networks info <name>` not needed:** Node networks are custom nodes, so the existing `describe` command shows their usage (parameters, return type). To see internal implementation, activate the network and use `query`.

3. **Renderer refresh on activation:** Yes, needed. The Rust API already handles this via `refresh_structure_designer_auto()` in `set_active_node_network()`.
