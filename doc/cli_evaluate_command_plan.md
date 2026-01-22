# CLI Evaluate Command Plan

## Overview

Add an `evaluate` command to atomcad-cli that outputs the textual result of evaluating a specific node. This provides AI agents with immediate feedback about node computation results without requiring visual inspection.

## Why This Is Useful

When an AI agent builds a node network, it currently has no way to verify intermediate results. The agent must:
1. Trust that the network was constructed correctly
2. Rely solely on screenshots (when implemented) for feedback

The `evaluate` command enables:
- **Verification of primitive calculations**: Confirm that an `expr` node computed the expected value
- **Atom/bond counting**: Check that an `atom_fill` produced the expected structure size
- **Type confirmation**: Verify that a node outputs the expected data type
- **Error detection**: See error messages when evaluation fails

## CLI Commands

```bash
# Brief output (default) - shows type and value for primitives, type name for complex types
atomcad-cli evaluate <node_id>

# Verbose output - shows detailed structure for complex types
atomcad-cli evaluate <node_id> --verbose
```

### Example Outputs

**Brief mode (default):**
```
$ atomcad-cli evaluate sphere1
Geometry

$ atomcad-cli evaluate count1
42

$ atomcad-cli evaluate atoms1
Atomic

$ atomcad-cli evaluate pos1
(5.000000, 10.000000, 3.000000)
```

**Verbose mode:**
```
$ atomcad-cli evaluate atoms1 --verbose
Atomic:
atoms: 968
bonds: 1372
frame_transform:
  translation: (0.000000, 0.000000, 0.000000)
  rotation: (0.000000, 0.000000, 0.000000, 1.000000)
first 10 atoms:
  [1] Z=6 pos=(-6.242250, -6.242250, -4.458750) depth=0.811 bonds=4
  [2] Z=6 pos=(-7.134000, -7.134000, -1.783500) depth=-0.000 bonds=4
  ... and 958 more atoms
first 10 bonds:
  1 -- 58 (order=1)
  ... and 1362 more bonds
```

## Context Window Considerations

- **Brief mode**: 1-5 tokens per node - safe for frequent use
- **Verbose mode for Atomic**: ~150 tokens - bounded (shows first 10 atoms/bonds)
- **Verbose mode for Geometry**: Unbounded - CSG tree can grow large

Recommendation: Agents should default to brief mode and use verbose selectively.

## Implementation Notes

The core functionality already exists in `NetworkResult`:

- `NetworkResult::to_display_string()` at [network_result.rs:445-482](rust/src/structure_designer/evaluator/network_result.rs#L445-L482) - brief output
- `NetworkResult::to_detailed_string()` at [network_result.rs:488-507](rust/src/structure_designer/evaluator/network_result.rs#L488-L507) - verbose output

Implementation requires:
1. Add `evaluate` subcommand to CLI parser in the server command handling
2. Look up node by ID in the current network
3. Call `to_display_string()` or `to_detailed_string()` based on `--verbose` flag
4. Return result string to client

The node lookup and evaluation infrastructure already exists in the `StructureDesigner` - this is primarily CLI wiring work.

---

## Detailed Implementation Plan

### Phase 1: API Types and Core Function

**Goal:** Add the types and core evaluation function that retrieves a node's result.

#### Step 1.1: Add API Response Type

**File:** `rust/src/api/structure_designer/structure_designer_api_types.rs`

Add a new struct to represent the evaluation result:

```rust
/// Result of evaluating a single node via CLI
#[derive(Debug, Clone)]
pub struct APINodeEvaluationResult {
    /// The node ID that was evaluated
    pub node_id: u64,
    /// The node type name (e.g., "cuboid", "atom_fill")
    pub node_type_name: String,
    /// The custom name if assigned, otherwise None
    pub custom_name: Option<String>,
    /// The output data type name (e.g., "Geometry", "Atomic", "Float")
    pub output_type: String,
    /// Brief display string (from to_display_string())
    pub display_string: String,
    /// Detailed string (from to_detailed_string()), only populated if verbose=true
    pub detailed_string: Option<String>,
    /// Whether the evaluation succeeded (no errors in this node's chain)
    pub success: bool,
    /// Error message if the node itself produced an error
    pub error_message: Option<String>,
}
```

#### Step 1.2: Add Core Evaluation Method

**File:** `rust/src/structure_designer/structure_designer.rs`

Add a method to evaluate a node by ID and return the result:

```rust
/// Evaluate a specific node and return its result for CLI inspection.
///
/// This triggers evaluation of the node (if not already cached) and returns
/// the NetworkResult converted to strings for display.
///
/// # Arguments
/// * `node_id` - The ID of the node to evaluate (can be u64 or string name)
/// * `verbose` - If true, include detailed output for complex types
///
/// # Returns
/// * `Ok(APINodeEvaluationResult)` - The evaluation result
/// * `Err(String)` - If node not found or network not active
pub fn evaluate_node_for_cli(
    &mut self,
    node_id: u64,
    verbose: bool,
) -> Result<APINodeEvaluationResult, String>
```

**Implementation approach:**
1. Check that an active network is set
2. Look up the node in the active network's `nodes` HashMap
3. Use `network_evaluator.evaluate()` to get the `NetworkResult` for the node's output pin (index 0)
4. Call `to_display_string()` for brief output
5. If verbose, also call `to_detailed_string()`
6. Check if result is `NetworkResult::Error` to set success/error fields
7. Return populated `APINodeEvaluationResult`

**Key consideration:** The node may not be in `displayed_node_ids`, so we need direct evaluation rather than pulling from the cached scene data.

#### Step 1.3: Add Node Lookup by Name

Support looking up nodes by their custom name (e.g., "sphere1") in addition to numeric ID:

```rust
/// Find a node ID by its custom name in the active network.
/// Returns the first matching node if multiple have the same name.
pub fn find_node_id_by_name(&self, name: &str) -> Option<u64>
```

This iterates `network.nodes` and checks each `node.custom_name`.

---

### Phase 2: API Exposure

**Goal:** Expose the evaluation function through the Flutter Rust Bridge API.

#### Step 2.1: Add API Function

**File:** `rust/src/api/structure_designer/structure_designer_api.rs`

```rust
/// Evaluate a node and return its result string.
///
/// # Arguments
/// * `node_identifier` - Either a numeric node ID or the node's custom name
/// * `verbose` - If true, return detailed output for complex types
///
/// # Returns
/// * `Ok(APINodeEvaluationResult)` - The evaluation result
/// * `Err(String)` - If node not found or evaluation fails
#[flutter_rust_bridge::frb(sync)]
pub fn evaluate_node(
    node_identifier: String,
    verbose: bool,
) -> Result<APINodeEvaluationResult, String> {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let designer = &mut cad_instance.structure_designer;

            // Try parsing as numeric ID first, then fall back to name lookup
            let node_id = node_identifier.parse::<u64>()
                .ok()
                .or_else(|| designer.find_node_id_by_name(&node_identifier))
                .ok_or_else(|| format!("Node not found: {}", node_identifier))?;

            designer.evaluate_node_for_cli(node_id, verbose)
        })
    }
}
```

#### Step 2.2: Regenerate FFI Bindings

After adding the API function, run:
```bash
flutter_rust_bridge_codegen generate
```

This generates the Dart bindings in `lib/src/rust/`.

---

### Phase 3: CLI Server Integration

**Goal:** Wire the evaluate command into the CLI server's command handling.

#### Step 3.1: Add Evaluate Command to CLI Server

**File:** `lib/structure_designer/cli/cli_server.dart` (or equivalent command handler)

The CLI server receives JSON commands from the CLI client. Add handling for the `evaluate` command:

```dart
case 'evaluate':
  final nodeIdentifier = command['node_id'] as String;
  final verbose = command['verbose'] as bool? ?? false;

  try {
    final result = evaluateNode(
      nodeIdentifier: nodeIdentifier,
      verbose: verbose,
    );

    // Format output based on verbosity
    if (verbose && result.detailedString != null) {
      return result.detailedString!;
    } else {
      return result.displayString;
    }
  } catch (e) {
    return 'Error: $e';
  }
```

#### Step 3.2: Update CLI Client Parser

**File:** `lib/main.dart` (CLI entry point) or dedicated CLI parser

Add argument parsing for the evaluate command:

```dart
// Parse: atomcad-cli evaluate <node_id> [--verbose]
if (args.length >= 2 && args[0] == 'evaluate') {
  final nodeId = args[1];
  final verbose = args.contains('--verbose') || args.contains('-v');

  sendCommand({
    'command': 'evaluate',
    'node_id': nodeId,
    'verbose': verbose,
  });
}
```

---

### Phase 4: Testing

**Goal:** Verify the evaluate command works correctly for all data types.

#### Step 4.1: Unit Tests for Core Function

**File:** `rust/tests/evaluate_node_tests.rs`

Test cases:
1. **Primitive types**: Evaluate nodes producing Float, Int, Bool, String, Vec3
2. **Complex types brief**: Evaluate Geometry and Atomic nodes, verify brief output
3. **Complex types verbose**: Verify detailed output includes expected fields
4. **Error propagation**: Evaluate a node with disconnected required inputs
5. **Node not found**: Verify proper error message
6. **Name lookup**: Test finding nodes by custom name

```rust
#[test]
fn test_evaluate_float_node() {
    let mut designer = create_test_designer();
    // Add an expr node that computes a float
    let node_id = add_expr_node(&mut designer, "42.5");

    let result = designer.evaluate_node_for_cli(node_id, false).unwrap();

    assert_eq!(result.output_type, "Float");
    assert_eq!(result.display_string, "42.500000");
    assert!(result.success);
}

#[test]
fn test_evaluate_atomic_verbose() {
    let mut designer = create_test_designer();
    // Add nodes to create a small atomic structure
    let atoms_id = build_diamond_cube(&mut designer);

    let result = designer.evaluate_node_for_cli(atoms_id, true).unwrap();

    assert_eq!(result.output_type, "Atomic");
    assert!(result.detailed_string.is_some());
    let detailed = result.detailed_string.unwrap();
    assert!(detailed.contains("atoms:"));
    assert!(detailed.contains("bonds:"));
}
```

#### Step 4.2: Integration Test via CLI

**File:** `integration_test/cli_evaluate_test.dart` (or shell script)

End-to-end test:
1. Load a test `.cnnd` file with known nodes
2. Run `atomcad-cli evaluate <node_id>`
3. Verify output matches expected string
4. Run with `--verbose` and verify detailed output

---

### Phase 5: Documentation and Polish

#### Step 5.1: Update atomCAD Skill Documentation

**File:** `.claude/skills/atomcad.md`

Add the evaluate command to the skill's available commands section:

```markdown
### evaluate
Evaluate a node and return its result.

**Usage:**
```
evaluate <node_id> [--verbose]
```

**Parameters:**
- `node_id`: The node's numeric ID or custom name
- `--verbose`: Include detailed output for complex types (atoms, geometry)

**Examples:**
```
evaluate sphere1           # Returns "Geometry"
evaluate count1            # Returns "42"
evaluate atoms1 --verbose  # Returns detailed atom/bond info
```
```

#### Step 5.2: Add Help Text

Ensure `atomcad-cli --help` and `atomcad-cli evaluate --help` show proper usage.

---

## File Summary

| File | Changes |
|------|---------|
| `rust/src/api/structure_designer/structure_designer_api_types.rs` | Add `APINodeEvaluationResult` struct |
| `rust/src/structure_designer/structure_designer.rs` | Add `evaluate_node_for_cli()` and `find_node_id_by_name()` |
| `rust/src/api/structure_designer/structure_designer_api.rs` | Add `evaluate_node()` API function |
| `lib/structure_designer/cli/cli_server.dart` | Add evaluate command handling |
| `lib/main.dart` | Add CLI argument parsing for evaluate |
| `rust/tests/evaluate_node_tests.rs` | New test file |
| `.claude/skills/atomcad.md` | Document new command |

---

## Dependencies Between Phases

```
Phase 1 (Types + Core)
    │
    ▼
Phase 2 (API Exposure) ──► flutter_rust_bridge_codegen generate
    │
    ▼
Phase 3 (CLI Integration)
    │
    ▼
Phase 4 (Testing) ◄────── Can start unit tests after Phase 1
    │
    ▼
Phase 5 (Documentation)
```

Phases 1-3 must be sequential. Phase 4 unit tests can begin after Phase 1. Phase 5 can be done in parallel with Phase 4.

---

## Estimated Complexity

- **Phase 1:** Medium - Core implementation, requires understanding evaluation context
- **Phase 2:** Low - Standard API wiring following existing patterns
- **Phase 3:** Low - Following existing CLI command patterns
- **Phase 4:** Medium - Need test fixtures for various data types
- **Phase 5:** Low - Documentation updates

Total: ~200-300 lines of new code across Rust and Dart.
