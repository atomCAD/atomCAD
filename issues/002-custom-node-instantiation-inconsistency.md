# Issue: Custom Node Listed but Cannot Be Instantiated via CLI

## Summary

Custom nodes appear in the `nodes` listing and can be described with `describe`, but attempting to create an instance via `edit` returns "Unknown node type" error.

## Severity

**Medium** - Custom nodes are a key feature for parametric design. Users cannot create instances of custom nodes via the CLI.

## Steps to Reproduce

```bash
# 1. Start atomCAD (ensure the application is running)

# 2. List available node types - custom nodes appear
cd c:/machine_phase_systems/flutter_cad
./atomcad-cli nodes
# Output includes:
# === Custom ===
#   sample

# 3. List only custom nodes
./atomcad-cli nodes --category=Custom
# Output:
# === Custom ===
#   sample

# 4. Describe the custom node - works fine
./atomcad-cli describe sample
# Output:
# Node: sample
# Category: Custom
# Description:
# Output: dynamic

# 5. Try to create an instance of the custom node - FAILS
./atomcad-cli edit --code="sample_inst = sample { visible: true }"
# Output:
# {"success":false,"nodes_created":[],"nodes_updated":[],"nodes_deleted":[],"connections_made":[],"errors":["Unknown node type: 'sample'"],"warnings":[]}
```

## Expected Behavior

Creating an instance of a listed custom node should succeed:
```json
{"success":true,"nodes_created":["sample_inst"],...}
```

## Actual Behavior

Returns error indicating the node type is unknown, despite:
1. Being listed in `nodes` output
2. Being describable via `describe`

## Analysis

The inconsistency suggests:
1. The `nodes` and `describe` commands query a node type registry that includes custom nodes
2. The `edit` command's parser or node instantiation logic uses a different lookup mechanism that doesn't include custom nodes
3. OR custom nodes require special handling/syntax that isn't documented

## Questions to Investigate

1. How are custom nodes registered? Is there a separate registry for built-in vs custom?
2. Does the edit command's text parser handle custom node types differently?
3. Is there additional syntax needed (e.g., namespacing) to instantiate custom nodes?
4. Are custom nodes meant to be created only through the UI?

## Relevant Files to Investigate

- `rust/src/api/` - CLI API implementation
- `rust/src/structure_designer/node_type_registry.rs` - Node type registry
- Text format parser (handles the edit --code syntax)
- Custom node loading/registration logic

## Workaround

Currently unknown. Users may need to use the GUI to instantiate custom nodes.

## Environment

- Platform: Windows
- CLI: atomcad-cli via bash wrapper
- Server port: 19847 (default)
