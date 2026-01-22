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
