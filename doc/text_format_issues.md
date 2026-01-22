# Text Format Issues for Dynamic Nodes

This document captures issues discovered while testing the atomcad-cli text format with dynamic nodes (`expr`, `parameter`) and array literal properties.

## Issue 1: Array Literal Properties Are Completely Ignored (CRITICAL)

**Severity:** Critical - blocks Mode 1 functionality for expr node

**Symptoms:**
```bash
# Attempt to set vertices on polygon
echo "poly = polygon { vertices: [(0, 0), (10, 0), (5, 10)] }" | atomcad-cli edit --replace
atomcad-cli query
# Result: poly = polygon { vertices: [(-1, -1), (1, -1), (0, 1)] }
# The vertices stayed at default!

# Attempt to set parameters on expr
echo "result = expr { expression: \"a + b\", parameters: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: Int }] }" | atomcad-cli edit --replace
atomcad-cli query
# Result: result = expr { expression: "a + b", parameters: [{ name: "x", data_type: Int }] }
# Only expression was updated, parameters stayed at default!
```

**Root Cause:**
In `network_editor.rs`, the `apply_literal_properties` function (lines 458-513) only processes `PropertyValue::Literal`:

```rust
for (prop_name, prop_value) in properties {
    if prop_name == "visible" { continue; }
    if let PropertyValue::Literal(text_value) = prop_value {  // <-- Only this branch handles literals
        literal_props.insert(prop_name.clone(), text_value.clone());
    }
    // Skip NodeRef, FunctionRef, Array - handled in connection pass  // <-- Arrays skipped!
}
```

When the parser encounters `[...]`, it creates `PropertyValue::Array(Vec<PropertyValue>)`, NOT `PropertyValue::Literal(TextValue::Array(...))`. So array properties are completely skipped.

**Affected Nodes:**
- `expr` - `parameters` property
- `polygon` - `vertices` property
- Any other node with array literal properties

**Suggested Fix:**
Modify `apply_literal_properties` to convert `PropertyValue::Array` to `TextValue::Array` when all elements are literals:

```rust
fn property_value_to_text_value(pv: &PropertyValue) -> Option<TextValue> {
    match pv {
        PropertyValue::Literal(tv) => Some(tv.clone()),
        PropertyValue::Array(items) => {
            let converted: Option<Vec<TextValue>> = items
                .iter()
                .map(|item| property_value_to_text_value(item))
                .collect();
            converted.map(TextValue::Array)
        }
        PropertyValue::NodeRef(_) | PropertyValue::FunctionRef(_) => None,
    }
}

// Then in apply_literal_properties:
for (prop_name, prop_value) in properties {
    if prop_name == "visible" { continue; }
    if let Some(text_value) = property_value_to_text_value(prop_value) {
        literal_props.insert(prop_name.clone(), text_value);
    }
}
```

---

## Issue 2: Spurious "Unknown Property" Warnings for Literal-Only Properties

**Severity:** Minor - cosmetic warning spam

**Symptoms:**
```bash
echo "p = parameter { param_name: \"size\", data_type: Float }" | atomcad-cli edit --replace
# Warnings:
#   "Unknown property 'param_name' on node type 'parameter'"
#   "Unknown property 'data_type' on node type 'parameter'"
# But the properties ARE saved correctly!
```

**Root Cause:**
The warning check in `apply_literal_properties` (lines 493-498) validates against `node_type.parameters`, which only contains wirable input pins. Literal-only properties (defined in `get_text_properties()` but not in `parameters`) are incorrectly flagged as unknown.

```rust
if !valid_params.is_empty() && !valid_params.contains(prop_name) {
    self.result.add_warning(format!(
        "Unknown property '{}' on node type '{}'",
        prop_name, node_type_name
    ));
}
```

**Suggested Fix:**
Also check against properties returned by `get_text_properties()`:

```rust
// Get both parameter names AND text property names
let text_prop_names: HashSet<String> = node.data
    .get_text_properties()
    .iter()
    .map(|(name, _)| name.clone())
    .collect();

// Only warn if property is in neither set
if !valid_params.is_empty()
    && !valid_params.contains(prop_name)
    && !text_prop_names.contains(prop_name) {
    self.result.add_warning(...);
}
```

---

## Issue 3: `describe` Output Shows `None` for Dynamic Types

**Severity:** Medium - confusing documentation

**Symptoms:**
```
atomcad-cli describe expr
# Shows:
#   parameters : [None]  [default: [{ name: "x", data_type: Int }], literal-only]
#   Output: None

atomcad-cli describe parameter
# Shows:
#   data_type : None    [default: Int, literal-only]
```

**Root Cause:**
`DataType::None` is displayed as the string "None", which is confusing. For dynamic-type nodes, this should indicate the type is determined at runtime.

**Suggested Fix:**
In `node_type_introspection.rs`, add a helper to format data types more clearly:

```rust
fn format_data_type_for_display(dt: &DataType) -> String {
    match dt {
        DataType::None => "dynamic".to_string(),
        DataType::Array(inner) if matches!(**inner, DataType::None) => "[dynamic]".to_string(),
        other => other.to_string(),
    }
}
```

---

## Issue 4: `describe` for expr Doesn't Show Dynamic Input Pins

**Severity:** Medium - incomplete documentation

**Symptoms:**
The `describe expr` output shows the literal-only configuration properties but doesn't explain how dynamic input pins work or show the default instance's actual pins.

**Current Output:**
```
Inputs:
  expression : String  [default: "x", literal-only]
  parameters : [None]  [default: [{ name: "x", data_type: Int }], literal-only]

Output: None
```

**Suggested Improvement:**
Show the default instance's actual input pins:

```
Configuration (literal-only):
  expression : String  [default: "x"]
  parameters : [...]   [defines input pins below]

Dynamic Inputs (based on default configuration):
  x : Int  [required]

Output: dynamic (inferred from expression; default: Int)

Usage Example:
  x_val = int { value: 5 }
  y_val = int { value: 3 }
  result = expr {
    expression: "x * 2 + y",
    parameters: [{ name: "x", data_type: Int }, { name: "y", data_type: Int }],
    x: x_val,
    y: y_val
  }
```

**Implementation:**
In `describe_node_type`, call `calculate_custom_node_type()` on the default instance and display both the configuration properties and the resulting dynamic pins separately.

---

## Summary

| Issue | Severity | Blocks Mode 1? | Fix Complexity |
|-------|----------|----------------|----------------|
| 1. Array literals ignored | Critical | Yes | Medium |
| 2. Spurious warnings | Minor | No | Easy |
| 3. `None` type display | Medium | No | Easy |
| 4. Missing dynamic pin docs | Medium | No | Medium |

**Recommended Priority:**
1. Fix Issue 1 first - it completely blocks the explicit Mode 1 syntax for expr
2. Fix Issue 3 - quick win for clarity
3. Fix Issue 2 - reduces noise
4. Fix Issue 4 - improves agent usability

---

## Test Commands for Verification

After fixes, these should work:

```bash
# Test 1: Array literal properties should be saved
echo "poly = polygon { vertices: [(0, 0), (10, 0), (5, 10)] }" | atomcad-cli edit --replace
atomcad-cli query
# Expected: poly = polygon { vertices: [(0, 0), (10, 0), (5, 10)] }

# Test 2: expr with multiple parameters should work
echo "x_val = int { value: 5 }
y_val = int { value: 3 }
result = expr { expression: \"x * 2 + y\", parameters: [{ name: \"x\", data_type: Int }, { name: \"y\", data_type: Int }], x: x_val, y: y_val, visible: true }" | atomcad-cli edit --replace
atomcad-cli query
# Expected: Both x and y parameters defined and wired

# Test 3: No spurious warnings for literal-only properties
echo "p = parameter { param_name: \"size\", data_type: Float }" | atomcad-cli edit --replace
# Expected: No warnings about 'param_name' or 'data_type'
```

---

## Required Reading Before Fixing

AI agents should read these files before attempting fixes:

### Background Context (read first)

| File | Purpose |
|------|---------|
| `.claude/skills/atomcad/skill.md` | Understand the CLI and text format syntax |
| `rust/src/structure_designer/node_data.rs` | Core `NodeData` trait with `get_text_properties()`, `set_text_properties()`, `calculate_custom_node_type()` |
| `rust/src/structure_designer/data_type.rs` | `DataType` enum and its `Display` implementation |

### Issue 1: Array Literals Ignored

| File | Lines | Why |
|------|-------|-----|
| `rust/src/structure_designer/text_format/network_editor.rs` | 458-513 | **Fix location**: `apply_literal_properties` function |
| `rust/src/structure_designer/text_format/parser.rs` | 628-708 | Understand how `PropertyValue::Array` vs `PropertyValue::Literal` are created |
| `rust/src/structure_designer/text_format/text_value.rs` | all | Understand `TextValue` enum structure |
| `rust/src/structure_designer/text_format/mod.rs` | all | Module exports and `PropertyValue` enum definition |

### Issue 2: Spurious Warnings

| File | Lines | Why |
|------|-------|-----|
| `rust/src/structure_designer/text_format/network_editor.rs` | 463-502 | **Fix location**: warning generation in `apply_literal_properties` |
| `rust/src/structure_designer/nodes/parameter.rs` | 87-117 | Example of `get_text_properties()` returning literal-only props |

### Issue 3: `None` Type Display

| File | Lines | Why |
|------|-------|-----|
| `rust/src/structure_designer/text_format/node_type_introspection.rs` | 132-275 | **Fix location**: `describe_node_type` function |
| `rust/src/structure_designer/data_type.rs` | all | `DataType::None` and `Display` impl |

### Issue 4: Dynamic Pin Documentation

| File | Lines | Why |
|------|-------|-----|
| `rust/src/structure_designer/text_format/node_type_introspection.rs` | 132-275 | **Fix location**: `describe_node_type` function |
| `rust/src/structure_designer/nodes/expr.rs` | 99-114 | How `calculate_custom_node_type` generates dynamic pins |
| `rust/src/structure_designer/nodes/parameter.rs` | 32-39 | Another example of `calculate_custom_node_type` |
| `rust/src/structure_designer/node_type.rs` | all | `NodeType` and `Parameter` structures |

### Testing

| File | Purpose |
|------|---------|
| `rust/AGENTS.md` | Testing conventions for the Rust codebase |
| `rust/tests/` | Location for new tests (never inline `#[cfg(test)]`) |
