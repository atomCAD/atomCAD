# atomCAD Skill Test Report

**Date:** 2026-01-20
**Tested by:** Claude (AI Assistant)
**Version:** skill.md v2.0

## Executive Summary

The atomCAD skill provides a functional CLI interface for interacting with atomCAD node networks. Core functionality works well, but there is one **critical bug** that significantly impacts usability, along with several areas for improvement.

---

## What Was Tested

### 1. Query Operations
- `atomcad-cli query` - Query current network state
- Result: **Working correctly**

### 2. Node Discovery
- `atomcad-cli nodes` - List all node types
- `atomcad-cli nodes --verbose` - List with descriptions
- `atomcad-cli nodes --category=<cat>` - Filter by category
- `atomcad-cli describe <node>` - Get detailed node info
- Result: **All working correctly**

### 3. Edit Operations
- Adding nodes with `edit --code="..."`
- Deleting nodes with `delete <id>`
- Modifying existing nodes
- Replace entire network with `--replace`
- Multiline input via stdin
- Wiring nodes together
- Result: **Functionally working, but with critical ID bug**

### 4. Boolean Operations
- diff (difference)
- union (attempted via documentation)
- Result: **Working when using correct node IDs**

### 5. Atomic Structure
- `atom_fill` - Convert geometry to atoms
- Result: **Working correctly**

### 6. Error Handling
- Invalid node types - **Good error message**
- Invalid categories - **Good error message with valid list**
- Non-existent node describe - **Good error message**
- Invalid property names - **Silently ignored (issue)**

---

## Bugs Found

### BUG 1: Node IDs Ignored (CRITICAL)

**Severity:** Critical
**Impact:** Makes wiring nodes together unreliable without querying first

**Description:**
When creating a node with a user-specified ID, the ID is ignored and an auto-generated ID based on the node type is used instead. However, the success response incorrectly reports the user-specified ID was created.

**Reproduction:**
```bash
$ atomcad-cli edit --code="mybox = cuboid { extent: (5, 5, 5) }"
# Response: {"success":true,"nodes_created":["mybox"],...}

$ atomcad-cli query
# Shows: cuboid1 = cuboid { extent: (5, 5, 5) }
```

The response says `"nodes_created":["mybox"]` but the actual node is named `cuboid1`.

**Consequences:**
1. When following up with wiring commands like `diff { base: [mybox] }`, the node reference fails
2. Users must query after every create to get the actual ID
3. The skill documentation examples suggest using custom IDs, which won't work as expected
4. Multi-statement commands fail because they reference the user-specified IDs

**Suggested Fix:**
Either:
- (A) Honor user-specified IDs when creating nodes
- (B) Return the actual generated ID in the success response (e.g., `"nodes_created":["cuboid1"]`)

Option (A) is preferred for better usability.

---

### BUG 2: Invalid Properties Silently Ignored

**Severity:** Low
**Impact:** Typos in property names go unnoticed

**Description:**
When specifying an invalid property name on a node, no error or warning is generated.

**Reproduction:**
```bash
$ atomcad-cli edit --code="s = sphere { invalid_property: 5 }"
# Response: {"success":true,...}  -- no warning
```

**Suggested Fix:**
Add a warning in the response for unknown properties.

---

### BUG 3: Multi-line Code in --code Flag Parse Error

**Severity:** Medium
**Impact:** Cannot create multiple nodes in a single command

**Description:**
When passing multi-line code via `--code`, newlines appear to be converted to HTML entities or similar, causing parse errors.

**Reproduction:**
```bash
$ atomcad-cli edit --replace --code="a = sphere { radius: 1 }
b = cuboid { extent: (2, 2, 2) }"
# Response: Parse error at line 1: Unexpected character: '&'
```

**Workaround:**
Use stdin piping: `echo "..." | atomcad-cli edit`

**Suggested Fix:**
Handle newlines correctly in command-line argument parsing.

---

## Issues and Observations

### Issue 1: expr Node Shows "Output: None"

The `describe expr` command shows `Output: None`, which is misleading since expr evaluates to dynamic types.

```
Output: None
```

**Suggestion:** Show "Output: (dynamic)" or document that output type depends on expression.

### Issue 2: Custom Node "sample" Has Empty Description

```bash
$ atomcad-cli describe sample
# Shows: Description: (empty), Output: None
```

This may be intentional (empty custom node), but could confuse users.

### Issue 3: Property Naming Inconsistency

Some properties in `atom_fill` have shortened names in the CLI vs. documentation:
- `rm_single` vs `remove_single_bond`
- `surf_recon` vs `reconstruct`
- `m_offset` vs `motif_offset`

The documentation says `passivate` but CLI shows `passivate` (this one matches).

---

## What Works Well

1. **Node Discovery** - The `nodes` and `describe` commands are excellent for learning what's available
2. **Verbose Mode** - `nodes --verbose` provides helpful brief descriptions
3. **Category Filtering** - `nodes --category=X` is useful for focusing on specific areas
4. **Describe Output** - Very detailed with parameters, types, defaults, and descriptions
5. **Error Messages** - Invalid node types and categories return helpful error messages with suggestions
6. **Query Output** - Clean, readable text format showing the network state
7. **Output Node** - Setting network output with `output <id>` works correctly
8. **Wiring** - When using correct IDs, wiring reports connections made

---

## Recommendations for Skill Documentation

### High Priority
1. **Add a "Known Issues" section** documenting the node ID behavior until fixed
2. **Remove multi-line --code examples** - Use stdin examples instead
3. **Add a note** that users should query after creating nodes to get actual IDs

### Medium Priority
4. **Add troubleshooting section** for common errors
5. **Document the property name mappings** (e.g., `rm_single` = `remove_single_bond`)
6. **Add example workflow** showing query -> create -> query -> wire pattern

### Low Priority
7. **Add section on REPL mode** with example session
8. **Document stdin termination** (empty line or ".")

---

## Test Summary

| Feature | Status | Notes |
|---------|--------|-------|
| query | Pass | Works correctly |
| nodes | Pass | All modes work |
| nodes --verbose | Pass | Good descriptions |
| nodes --category | Pass | With valid category error handling |
| describe | Pass | Detailed output |
| edit (create) | Partial | Works but ID bug |
| edit (delete) | Pass | Works with actual IDs |
| edit (modify) | Pass | Works with actual IDs |
| edit --replace | Partial | Works but ID bug |
| stdin input | Pass | Works for multi-line |
| wiring | Partial | Works if using actual IDs |
| output node | Pass | Works correctly |
| error handling | Partial | Some errors silent |

---

## Conclusion

The atomCAD skill provides valuable CLI access to atomCAD's node network capabilities. The core functionality is solid, but the **node ID bug is a critical issue** that makes the skill frustrating to use without constant querying. Once this is fixed, the skill will be highly effective for AI assistants to programmatically interact with atomCAD.

The documentation (skill.md) is comprehensive and well-written, but needs updating to reflect actual CLI behavior or the CLI needs fixing to match the documented behavior.
