# Parameter Rename Wire Preservation Fix

## IMPORTANT: Test-Driven Development Approach

**This fix MUST follow a strict TDD (Test-Driven Development) approach.**

### Implementation Order

```
1. FIRST: Write ALL tests (working scenarios + broken scenarios)
         ↓
2. SECOND: Run tests, verify expected results:
         - Working scenario tests (S1-S6, E1-E5): MUST PASS
         - Broken scenario tests (S8, E7): MUST FAIL
         ↓
3. THIRD: Implement the fix (Steps 1-11 in Fix Strategy)
         ↓
4. FOURTH: Run tests again, verify:
         - ALL tests MUST PASS
```

### Why This Order Matters

1. **Writing tests first** ensures we understand the problem completely before coding
2. **Verifying working tests pass** confirms our test infrastructure is correct
3. **Verifying broken tests fail** confirms we're testing the actual bug
4. **All tests passing after fix** proves the fix works without breaking existing behavior

### Expected Test Results By Phase

| Phase | Working Scenarios (S1-S6, E1-E5) | Broken Scenarios (S8, E7) |
|-------|----------------------------------|---------------------------|
| After writing tests, before fix | ✅ PASS | ❌ FAIL |
| After implementing fix | ✅ PASS | ✅ PASS |

**DO NOT proceed to implementation until you have verified the test results in the "before fix" phase match the expected results above.**

---

## Problem Statement

When a parameter of a node network or an expr node is **renamed**, wires connected to that parameter in parent networks are incorrectly disconnected. This happens because the wire preservation mechanism uses **parameter names** as identifiers, so when a name changes, the system cannot match the old parameter to the new one.

### Example: Subnetwork Parameter Rename

```
Network "MyFilter" has parameter "size: Float"
Network "main" uses MyFilter with a wire connected to "size"

User renames "size" to "length" in MyFilter

Result: Wire in "main" is disconnected (BUG - should be preserved)
```

### Example: Expr Node Parameter Rename

```
expr1 = expr { parameters: [{name: "x", data_type: Int}], expression: "x * 2", x: int1 }

User renames parameter "x" to "input"

Result: Wire from int1 is disconnected (BUG - should be preserved)
```

---

## Current Architecture

### Key Data Structures

**Parameter Definition (node_type.rs:10-13):**
```rust
pub struct Parameter {
    pub name: String,      // Only identifier - NO ID!
    pub data_type: DataType,
}
```

**Subnetwork Parameter Node Data (nodes/parameter.rs:17-26):**
```rust
pub struct ParameterData {
    pub param_index: usize,        // Position in parent's arguments array
    pub param_name: String,        // Name shown to users
    pub data_type: DataType,
    pub sort_order: i32,           // Determines parameter ordering
    pub data_type_str: Option<String>,
    pub error: Option<String>,
    // NO persistent ID!
}
```

**Expr Node Parameter (nodes/expr.rs:22-27):**
```rust
pub struct ExprParameter {
    pub name: String,              // Only identifier - NO ID!
    pub data_type: DataType,
    pub data_type_str: Option<String>,
}
```

**Wire Storage (node_network.rs:36-46):**
```rust
pub struct Argument {
    // Maps source_node_id -> output_pin_index
    pub argument_output_pins: HashMap<u64, i32>,
}

pub struct Wire {
    pub source_node_id: u64,
    pub source_output_pin_index: i32,
    pub destination_node_id: u64,
    pub destination_argument_index: usize,  // Index into arguments array
}
```

### Wire Preservation Mechanisms

#### Mechanism 1: Subnetwork Interface Changes (network_validator.rs:29-84)

When a subnetwork's parameters change, `repair_call_sites_for_network()` updates all parent networks:

```rust
fn repair_call_sites_for_network(
    network_name: &str,
    old_parameters: &[Parameter],
    new_parameters: &[Parameter],
    node_type_registry: &mut NodeTypeRegistry,
) {
    // Build mapping: parameter NAME -> old index
    let old_param_map: HashMap<&str, usize> = old_parameters
        .iter()
        .enumerate()
        .map(|(idx, param)| (param.name.as_str(), idx))  // Uses NAME!
        .collect();

    // For each new parameter, find old parameter by NAME
    for new_param in new_parameters {
        if let Some(&old_idx) = old_param_map.get(new_param.name.as_str()) {
            // Preserve wire from old index
            new_arguments.push(node.arguments[old_idx].clone());
        } else {
            // Name not found - wire is LOST
            new_arguments.push(Argument::new());
        }
    }
}
```

#### Mechanism 2: Expr/Custom Node Type Changes (node_network.rs:107-145)

When an expr node's parameters change, `set_custom_node_type()` preserves wires:

```rust
pub fn set_custom_node_type(&mut self, custom_node_type: Option<NodeType>, refresh_args: bool) {
    // ...
    for (new_index, new_param) in new_node_type.parameters.iter().enumerate() {
        // Find matching parameter NAME in old node type
        if let Some(old_index) = old_node_type.parameters.iter()
            .position(|old_param| old_param.name == new_param.name) {  // Uses NAME!
            new_arguments[new_index] = self.arguments[old_index].clone();
        }
        // If name not found, wire is LOST (new empty Argument)
    }
}
```

### Why Node Network Renaming Works (For Reference)

Node networks can be renamed without breaking wires because:
1. Wires reference nodes by **ID** (`source_node_id`, `destination_node_id`), not by name
2. Node names (`custom_name`) are purely for display
3. When a network type is renamed, all nodes using it get their `node_type_name` updated

This is the pattern we need to follow for parameters.

---

## Working Use Cases (Must Not Break)

### Subnetwork Parameter Use Cases

| ID | Use Case | Before | Operation | After | Expected Result |
|----|----------|--------|-----------|-------|-----------------|
| S1 | Add parameter at end | `[size: Float]` | Add `radius: Int` (sort_order=1) | `[size, radius]` | size wire preserved, radius empty |
| S2 | Add parameter in middle | `[size, radius]` (sort 0,2) | Add `depth` (sort_order=1) | `[size, depth, radius]` | size and radius wires repositioned correctly |
| S3 | Remove parameter | `[size, radius]` | Delete radius param node | `[size]` | size wire preserved, radius wire dropped |
| S4 | Reorder via sort_order | `[size, radius]` (sort 0,1) | Change radius sort to -1 | `[radius, size]` | Both wires follow their parameters to new positions |
| S5 | Change parameter type | `[size: Float]` | Change to `[size: Int]` | `[size: Int]` | Wire preserved (type error caught later in validation) |
| S6 | Multiple parents | Networks A and B both use MyFilter | Add param to MyFilter | Both updated | Both A and B get new empty argument |

### Expr Node Parameter Use Cases

| ID | Use Case | Before | Operation | After | Expected Result |
|----|----------|--------|-----------|-------|-----------------|
| E1 | Add parameter | `[x: Int]` | Add y parameter | `[x, y]` | x wire preserved, y empty |
| E2 | Remove parameter | `[x, y]` | Remove y | `[x]` | x wire preserved, y wire dropped |
| E3 | Change parameter type | `[x: Int]` | Change to `[x: Float]` | `[x: Float]` | Wire preserved |
| E4 | Change expression only | expr: `x * 2` | Change to `x + 1` | `x + 1` | All wires preserved (parameters unchanged) |
| E5 | Reorder parameters | `[x, y]` | Reorder to `[y, x]` | `[y, x]` | Wires follow parameters to new positions |

---

## Non-Working Use Cases (To Be Fixed)

| ID | Use Case | Before | Operation | After | Current (Bug) | Expected (Fixed) |
|----|----------|--------|-----------|-------|---------------|------------------|
| S8 | Rename subnetwork parameter | `[size: Float]` | Rename to `length` | `[length: Float]` | Wire LOST | Wire PRESERVED |
| E7 | Rename expr parameter | `[x: Int]` | Rename to `input` | `[input: Int]` | Wire LOST | Wire PRESERVED |

---

## Test Plan

**REMINDER: Write ALL tests FIRST, before any implementation changes.**

Create file: `rust/tests/structure_designer/parameter_wire_preservation_test.rs`

### Step 1: Write All Tests

Write all the tests below in a single commit. Do not modify any implementation code yet.

### Step 2: Run Tests and Verify Initial State

```bash
cd rust && cargo test parameter_wire_preservation
```

**Expected results before any fix:**
- Tests `test_subnetwork_add_parameter_*`, `test_subnetwork_remove_*`, `test_subnetwork_reorder_*`, `test_subnetwork_change_*`, `test_subnetwork_multiple_*` → **PASS**
- Tests `test_expr_add_*`, `test_expr_remove_*`, `test_expr_change_*`, `test_expr_reorder_*` → **PASS**
- Tests `test_subnetwork_rename_parameter_*` → **FAIL** (this is the bug we're fixing)
- Tests `test_expr_rename_parameter_*` → **FAIL** (this is the bug we're fixing)

**If the "working scenario" tests fail, your test code is wrong. Fix the tests first.**
**If the "broken scenario" tests pass, the bug may have been fixed elsewhere or your test isn't testing the right thing.**

### Step 3: Implement the Fix

Only after verifying the test results above, proceed to the Fix Strategy section.

### Step 4: Verify All Tests Pass

```bash
cd rust && cargo test parameter_wire_preservation
```

**All tests must pass. If any test fails, debug and fix before considering the implementation complete.**

---

### Test Infrastructure

```rust
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::nodes::parameter::ParameterData;
use rust_lib_flutter_cad::structure_designer::nodes::expr::ExprData;
use glam::f64::DVec2;

fn setup_designer() -> StructureDesigner {
    StructureDesigner::new()
}

/// Helper to get a node's argument connection count at a given parameter index
fn get_wire_count(designer: &StructureDesigner, network_name: &str, node_id: u64, param_index: usize) -> usize {
    let network = designer.node_type_registry.node_networks.get(network_name).unwrap();
    let node = network.nodes.get(&node_id).unwrap();
    node.arguments.get(param_index).map(|a| a.argument_output_pins.len()).unwrap_or(0)
}

/// Helper to check if a specific wire exists
fn has_wire_from(designer: &StructureDesigner, network_name: &str, dest_node_id: u64, param_index: usize, source_node_id: u64) -> bool {
    let network = designer.node_type_registry.node_networks.get(network_name).unwrap();
    let node = network.nodes.get(&dest_node_id).unwrap();
    node.arguments.get(param_index)
        .map(|a| a.argument_output_pins.contains_key(&source_node_id))
        .unwrap_or(false)
}
```

### Tests for Working Subnetwork Scenarios (MUST PASS before and after fix)

```rust
#[test]
fn test_subnetwork_add_parameter_preserves_existing_wires() {
    // S1: Create subnetwork with one parameter, use it in main, add second parameter
    // Verify: original wire preserved, new parameter has no wire
}

#[test]
fn test_subnetwork_add_parameter_in_middle_repositions_wires() {
    // S2: Create subnetwork with params at sort_order 0 and 2, add param at sort_order 1
    // Verify: wires follow their parameters to correct new indices
}

#[test]
fn test_subnetwork_remove_parameter_disconnects_wire() {
    // S3: Create subnetwork with two parameters, wire both, delete one parameter node
    // Verify: remaining parameter's wire preserved, deleted parameter's wire gone
}

#[test]
fn test_subnetwork_reorder_parameters_wires_follow_names() {
    // S4: Create subnetwork with two parameters, wire both, swap sort_order
    // Verify: wires follow parameters to new positions
}

#[test]
fn test_subnetwork_change_parameter_type_preserves_wire() {
    // S5: Create subnetwork with Float parameter, wire it, change to Int
    // Verify: wire preserved (validation may fail but wire exists)
}

#[test]
fn test_subnetwork_multiple_parents_all_repaired() {
    // S6: Create subnetwork, use in two different parent networks, modify parameters
    // Verify: both parent networks are updated correctly
}
```

### Tests for Working Expr Node Scenarios (MUST PASS before and after fix)

```rust
#[test]
fn test_expr_add_parameter_preserves_existing_wires() {
    // E1: Create expr with one parameter, wire it, add second parameter
    // Verify: original wire preserved
}

#[test]
fn test_expr_remove_parameter_disconnects_wire() {
    // E2: Create expr with two parameters, wire both, remove one
    // Verify: remaining wire preserved
}

#[test]
fn test_expr_change_parameter_type_preserves_wire() {
    // E3: Create expr with Int parameter, wire it, change to Float
    // Verify: wire preserved
}

#[test]
fn test_expr_change_expression_only_preserves_all_wires() {
    // E4: Create expr, wire parameters, change only expression text
    // Verify: all wires preserved
}

#[test]
fn test_expr_reorder_parameters_wires_follow_names() {
    // E5: Create expr with [x, y], wire both, reorder to [y, x]
    // Verify: wires follow parameters
}
```

### Tests for Broken Scenarios (MUST FAIL before fix, MUST PASS after fix)

```rust
#[test]
fn test_subnetwork_rename_parameter_preserves_wire() {
    // S8: Create subnetwork with parameter "size", wire it in parent, rename to "length"
    // Verify: wire is preserved after rename

    let mut designer = setup_designer();

    // Create subnetwork "MyFilter" with parameter "size"
    designer.add_node_network("MyFilter");
    designer.set_active_node_network_name(Some("MyFilter".to_string()));
    let param_id = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    // Set param_name to "size" via text properties or direct manipulation

    // Create main network that uses MyFilter
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let int_id = designer.add_node("int", DVec2::new(0.0, 0.0));
    let filter_id = designer.add_node("MyFilter", DVec2::new(100.0, 0.0));
    designer.connect_nodes(int_id, 0, filter_id, 0);  // Wire int to "size" parameter

    // Verify wire exists before rename
    assert!(has_wire_from(&designer, "main", filter_id, 0, int_id));

    // Rename parameter from "size" to "length"
    designer.set_active_node_network_name(Some("MyFilter".to_string()));
    // Update param_name via text properties

    // Verify wire still exists after rename (THIS WILL FAIL BEFORE FIX)
    assert!(has_wire_from(&designer, "main", filter_id, 0, int_id),
        "Wire should be preserved after parameter rename");
}

#[test]
fn test_expr_rename_parameter_preserves_wire() {
    // E7: Create expr with parameter "x", wire it, rename to "input"
    // Verify: wire is preserved after rename

    let mut designer = setup_designer();
    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));

    // Create int node as wire source
    let int_id = designer.add_node("int", DVec2::new(0.0, 0.0));

    // Create expr node with parameter "x"
    let expr_id = designer.add_node("expr", DVec2::new(100.0, 0.0));

    // Wire int to expr's "x" parameter (index 0)
    designer.connect_nodes(int_id, 0, expr_id, 0);

    // Verify wire exists before rename
    assert!(has_wire_from(&designer, "main", expr_id, 0, int_id));

    // Rename parameter from "x" to "input" via set_text_properties
    // (Update ExprData.parameters[0].name)

    // Verify wire still exists after rename (THIS WILL FAIL BEFORE FIX)
    assert!(has_wire_from(&designer, "main", expr_id, 0, int_id),
        "Wire should be preserved after expr parameter rename");
}
```

---

## Fix Strategy

> **PREREQUISITE:** Before starting this section, you MUST have:
> 1. Written all tests from the Test Plan section
> 2. Verified that working scenario tests PASS
> 3. Verified that broken scenario tests FAIL
>
> **DO NOT proceed with implementation until these prerequisites are met.**

### Overview

Add persistent IDs to parameters, similar to how nodes have IDs. Use these IDs for wire preservation instead of names.

### Step 1: Add ID to Parameter struct

**File: `rust/src/structure_designer/node_type.rs`**

```rust
#[derive(Clone, PartialEq)]
pub struct Parameter {
    pub id: Option<u64>,       // NEW: Persistent identifier
    pub name: String,
    pub data_type: DataType,
}
```

Update all places that construct `Parameter` to include `id: None` initially for backwards compatibility.

### Step 2: Add ID to ParameterData

**File: `rust/src/structure_designer/nodes/parameter.rs`**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterData {
    pub param_id: Option<u64>,     // NEW: Persistent identifier
    pub param_index: usize,
    pub param_name: String,
    pub data_type: DataType,
    pub sort_order: i32,
    pub data_type_str: Option<String>,
    #[serde(skip)]
    pub error: Option<String>,
}
```

Update `get_node_type()` default creator and `get_text_properties`/`set_text_properties`.

### Step 3: Add ID to ExprParameter

**File: `rust/src/structure_designer/nodes/expr.rs`**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExprParameter {
    pub id: Option<u64>,           // NEW: Persistent identifier
    pub name: String,
    pub data_type: DataType,
    pub data_type_str: Option<String>,
}
```

Update `get_text_properties`/`set_text_properties` to handle ID.

### Step 4: Add next_param_id counter to NodeNetwork

**File: `rust/src/structure_designer/node_network.rs`**

```rust
pub struct NodeNetwork {
    // ... existing fields ...
    pub next_param_id: u64,        // NEW: Counter for generating unique param IDs
}
```

Initialize to 1 in `NodeNetwork::new()`.

### Step 5: Generate IDs for new parameters

**File: `rust/src/structure_designer/structure_designer.rs`**

When creating a parameter node, assign a unique `param_id`:

```rust
// In add_node(), after special handling for parameter nodes:
if node_type_name == "parameter" {
    if let Some(param_data) = node_data.as_any_mut().downcast_mut::<ParameterData>() {
        param_data.param_id = Some(node_network.next_param_id);
        node_network.next_param_id += 1;
        // ... existing param_name and sort_order assignment ...
    }
}
```

### Step 6: Propagate ID in validate_parameters

**File: `rust/src/structure_designer/network_validator.rs`**

Update `validate_parameters()` to propagate IDs from ParameterData to the network's NodeType:

```rust
// In validate_parameters(), when building network.node_type.parameters:
network.node_type.parameters = parameter_nodes.iter().map(|(_, param_data)| {
    Parameter {
        id: param_data.param_id,   // NEW: Propagate ID
        name: param_data.param_name.clone(),
        data_type: param_data.data_type.clone(),
    }
}).collect();
```

### Step 7: Update repair_call_sites_for_network to use IDs

**File: `rust/src/structure_designer/network_validator.rs`**

```rust
fn repair_call_sites_for_network(
    network_name: &str,
    old_parameters: &[Parameter],
    new_parameters: &[Parameter],
    node_type_registry: &mut NodeTypeRegistry,
) {
    // Build mapping: parameter ID -> old index (primary)
    let old_param_id_map: HashMap<u64, usize> = old_parameters
        .iter()
        .enumerate()
        .filter_map(|(idx, param)| param.id.map(|id| (id, idx)))
        .collect();

    // Build mapping: parameter name -> old index (fallback for backwards compat)
    let old_param_name_map: HashMap<&str, usize> = old_parameters
        .iter()
        .enumerate()
        .map(|(idx, param)| (param.name.as_str(), idx))
        .collect();

    // ... in the loop over new_parameters:
    for new_param in new_parameters {
        // First try ID-based matching
        if let Some(new_id) = new_param.id {
            if let Some(&old_idx) = old_param_id_map.get(&new_id) {
                if old_idx < node.arguments.len() {
                    new_arguments.push(node.arguments[old_idx].clone());
                    continue;
                }
            }
        }

        // Fall back to name-based matching
        if let Some(&old_idx) = old_param_name_map.get(new_param.name.as_str()) {
            if old_idx < node.arguments.len() {
                new_arguments.push(node.arguments[old_idx].clone());
                continue;
            }
        }

        // No match found - new empty argument
        new_arguments.push(Argument::new());
    }
}
```

### Step 8: Propagate ID in ExprData.calculate_custom_node_type

**File: `rust/src/structure_designer/nodes/expr.rs`**

```rust
fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
    let mut custom_node_type = base_node_type.clone();

    custom_node_type.parameters = self.parameters.iter()
        .map(|expr_param| Parameter {
            id: expr_param.id,              // NEW: Propagate ID
            name: expr_param.name.clone(),
            data_type: expr_param.data_type.clone(),
        })
        .collect();

    // ... rest unchanged ...
}
```

### Step 9: Update set_custom_node_type to use IDs

**File: `rust/src/structure_designer/node_network.rs`**

```rust
pub fn set_custom_node_type(&mut self, custom_node_type: Option<NodeType>, refresh_args: bool) {
    if let Some(ref new_node_type) = custom_node_type {
        // Check if we can preserve existing arguments
        let can_preserve = if let Some(ref old_node_type) = self.custom_node_type {
            // Check if parameters have same IDs (or names if no IDs) in same order
            old_node_type.parameters.len() == new_node_type.parameters.len() &&
            old_node_type.parameters.iter()
                .zip(new_node_type.parameters.iter())
                .all(|(old_param, new_param)| {
                    // Match by ID if both have IDs, otherwise by name
                    match (old_param.id, new_param.id) {
                        (Some(old_id), Some(new_id)) => old_id == new_id,
                        _ => old_param.name == new_param.name,
                    }
                })
        } else {
            false
        };

        if (!refresh_args) || can_preserve {
            // Keep existing arguments
        } else {
            // Parameters changed, need to rebuild arguments array
            let mut new_arguments = vec![Argument::new(); new_node_type.parameters.len()];

            if let Some(ref old_node_type) = self.custom_node_type {
                // Build ID map for old parameters
                let old_id_map: HashMap<u64, usize> = old_node_type.parameters.iter()
                    .enumerate()
                    .filter_map(|(idx, p)| p.id.map(|id| (id, idx)))
                    .collect();

                for (new_index, new_param) in new_node_type.parameters.iter().enumerate() {
                    // First try ID-based matching
                    if let Some(new_id) = new_param.id {
                        if let Some(&old_index) = old_id_map.get(&new_id) {
                            if old_index < self.arguments.len() {
                                new_arguments[new_index] = self.arguments[old_index].clone();
                                continue;
                            }
                        }
                    }

                    // Fall back to name-based matching
                    if let Some(old_index) = old_node_type.parameters.iter()
                        .position(|old_param| old_param.name == new_param.name) {
                        if old_index < self.arguments.len() {
                            new_arguments[new_index] = self.arguments[old_index].clone();
                        }
                    }
                }
            }

            self.arguments = new_arguments;
        }
    }
    self.custom_node_type = custom_node_type;
}
```

### Step 10: Handle ID generation for expr parameters

**File: `rust/src/structure_designer/nodes/expr.rs`**

In `set_text_properties`, preserve IDs for existing parameters and generate new ones:

```rust
fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
    // ... expression handling ...

    if let Some(TextValue::Array(params_arr)) = props.get("parameters") {
        let mut new_params = Vec::new();
        let mut next_id = self.parameters.iter()
            .filter_map(|p| p.id)
            .max()
            .unwrap_or(0) + 1;

        for param_val in params_arr {
            if let TextValue::Object(obj) = param_val {
                let name = /* parse name */;
                let data_type = /* parse data_type */;
                let data_type_str = /* parse data_type_str */;

                // Try to find existing parameter with same name to preserve its ID
                let id = self.parameters.iter()
                    .find(|p| p.name == name)
                    .and_then(|p| p.id)
                    .or_else(|| {
                        let id = next_id;
                        next_id += 1;
                        Some(id)
                    });

                new_params.push(ExprParameter { id, name, data_type, data_type_str });
            }
        }
        self.parameters = new_params;
    }
    // ...
}
```

### Step 11: Update serialization for backwards compatibility

**File: `rust/src/structure_designer/serialization/node_networks_serialization.rs`**

During deserialization, if a parameter/network lacks IDs (old file format), generate them:

```rust
// After loading a network, ensure all parameters have IDs
fn migrate_parameter_ids(network: &mut NodeNetwork) {
    let mut next_id = network.next_param_id;

    for (_, node) in network.nodes.iter_mut() {
        if node.node_type_name == "parameter" {
            if let Some(param_data) = node.data.as_any_mut().downcast_mut::<ParameterData>() {
                if param_data.param_id.is_none() {
                    param_data.param_id = Some(next_id);
                    next_id += 1;
                }
            }
        }
    }

    network.next_param_id = next_id;

    // Also update the node_type.parameters to have matching IDs
    // (This requires re-running validate_parameters or similar)
}
```

---

## Files to Modify Summary

| File | Changes |
|------|---------|
| `rust/src/structure_designer/node_type.rs` | Add `id: Option<u64>` to `Parameter` |
| `rust/src/structure_designer/nodes/parameter.rs` | Add `param_id: Option<u64>` to `ParameterData`, update text properties |
| `rust/src/structure_designer/nodes/expr.rs` | Add `id: Option<u64>` to `ExprParameter`, update text properties and calculate_custom_node_type |
| `rust/src/structure_designer/node_network.rs` | Add `next_param_id` to `NodeNetwork`, update `set_custom_node_type` |
| `rust/src/structure_designer/network_validator.rs` | Update `validate_parameters` and `repair_call_sites_for_network` to use IDs |
| `rust/src/structure_designer/structure_designer.rs` | Generate param_id when creating parameter nodes |
| `rust/src/structure_designer/serialization/node_networks_serialization.rs` | Migration for old files without IDs |
| `rust/tests/structure_designer/parameter_wire_preservation_test.rs` | NEW: All tests described above |

---

## Verification Checklists

### Pre-Implementation Checklist (MUST complete before writing any fix code)

1. [ ] All tests from Test Plan written in `rust/tests/structure_designer/parameter_wire_preservation_test.rs`
2. [ ] Ran `cargo test parameter_wire_preservation`
3. [ ] Working scenario tests (S1-S6, E1-E5) all PASS
4. [ ] Broken scenario tests (S8, E7) all FAIL
5. [ ] Committed test file with message "Add parameter wire preservation tests (TDD)"

### Post-Implementation Checklist (MUST complete after fix)

1. [ ] All existing tests pass (`cargo test`)
2. [ ] All new tests pass, including rename tests (`cargo test parameter_wire_preservation`)
3. [ ] Old .cnnd files load correctly (backwards compatibility)
4. [ ] Newly saved files include parameter IDs
5. [ ] Round-trip serialization preserves IDs
6. [ ] No clippy warnings (`cargo clippy`)
7. [ ] Committed fix with message describing the change
