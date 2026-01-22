# Text Format Fixes - Implementation Plan

This document provides a phased implementation guide for fixing the issues documented in [text_format_issues.md](text_format_issues.md).

---

## Overview

| Phase | Issues | Focus | Complexity | Blocking? |
|-------|--------|-------|------------|-----------|
| **Phase 1** | #1 | Array literal properties ignored | Medium | Yes - blocks Mode 1 |
| **Phase 2** | #2 | Spurious warnings for literal-only props | Easy | No |
| **Phase 3** | #3 + #4 | Improve `describe` for dynamic nodes | Medium | No |

**Dependency Graph:**
```
Phase 1 ──────┬──────> Phase 3
              │
Phase 2 ──────┘
```
Phases 1 and 2 are independent and can be done in parallel. Phase 3 can proceed after either.

---

## Phase 1: Fix Array Literal Properties

### Problem Summary

When the parser encounters array syntax like `vertices: [(0, 0), (10, 0), (5, 10)]`, it creates `PropertyValue::Array(Vec<PropertyValue>)`. However, `apply_literal_properties` only handles `PropertyValue::Literal(TextValue)`, causing array properties to be silently ignored.

**Affected nodes:** `expr` (parameters), `polygon` (vertices), any node with array literal properties.

### Files to Modify

| File | Lines | Change |
|------|-------|--------|
| `rust/src/structure_designer/text_format/network_editor.rs` | 458-513 | Add recursive conversion of `PropertyValue::Array` → `TextValue::Array` |

### Implementation Steps

#### Step 1.1: Add helper function

Add a new helper function before `apply_literal_properties` (around line 456):

```rust
/// Recursively converts a PropertyValue to a TextValue if all nested values are literals.
/// Returns None if any nested value is a NodeRef or FunctionRef (these are handled in the connection pass).
fn property_value_to_text_value(pv: &PropertyValue) -> Option<TextValue> {
    match pv {
        PropertyValue::Literal(tv) => Some(tv.clone()),
        PropertyValue::Array(items) => {
            let converted: Option<Vec<TextValue>> = items
                .iter()
                .map(property_value_to_text_value)
                .collect();
            converted.map(TextValue::Array)
        }
        PropertyValue::NodeRef(_) | PropertyValue::FunctionRef(_) => None,
    }
}
```

#### Step 1.2: Update `apply_literal_properties`

Replace the loop body (lines 485-502) with:

```rust
for (prop_name, prop_value) in properties {
    // Skip special properties
    if prop_name == "visible" {
        continue;
    }

    // Try to convert PropertyValue to TextValue (handles literals and arrays of literals)
    if let Some(text_value) = property_value_to_text_value(prop_value) {
        // Warn about unknown properties
        if !valid_params.is_empty() && !valid_params.contains(prop_name) {
            self.result.add_warning(format!(
                "Unknown property '{}' on node type '{}'",
                prop_name, node_type_name
            ));
        }
        literal_props.insert(prop_name.clone(), text_value);
    }
    // Skip NodeRef, FunctionRef, and arrays containing them - handled in connection pass
}
```

### Testing

Create new test in `rust/tests/structure_designer/text_format_test.rs`:

```rust
#[test]
fn test_edit_array_literal_properties() {
    let registry = create_test_registry();
    let mut network = create_test_network();

    let result = edit_network(&mut network, &registry, r#"
        poly = polygon { vertices: [(0, 0), (10, 0), (5, 10)], visible: true }
    "#, true);

    assert!(result.success, "Edit should succeed: {:?}", result.errors);

    // Verify the vertices were actually set
    let serialized = serialize_network(&network, &registry);
    assert!(serialized.contains("(0, 0)"), "Should contain first vertex");
    assert!(serialized.contains("(10, 0)"), "Should contain second vertex");
    assert!(serialized.contains("(5, 10)"), "Should contain third vertex");
}

#[test]
fn test_edit_expr_with_parameters_array() {
    let registry = create_test_registry();
    let mut network = create_test_network();

    let result = edit_network(&mut network, &registry, r#"
        result = expr {
            expression: "x + y",
            parameters: [
                { name: "x", data_type: Int },
                { name: "y", data_type: Int }
            ]
        }
    "#, true);

    assert!(result.success, "Edit should succeed: {:?}", result.errors);

    // Verify the expression and parameters were set
    let serialized = serialize_network(&network, &registry);
    assert!(serialized.contains("expression: \"x + y\""), "Should contain expression");
    assert!(serialized.contains("name: \"x\""), "Should contain parameter x");
    assert!(serialized.contains("name: \"y\""), "Should contain parameter y");
}
```

### Verification Command

```bash
# After fix, this should work:
echo "poly = polygon { vertices: [(0, 0), (10, 0), (5, 10)] }" | atomcad-cli edit --replace
atomcad-cli query
# Expected: poly = polygon { vertices: [(0, 0), (10, 0), (5, 10)] }
```

### Done Criteria

- [ ] `PropertyValue::Array` with all-literal elements is converted to `TextValue::Array`
- [ ] Mixed arrays (literals + refs) fall through to connection pass
- [ ] New tests pass
- [ ] `cargo test text_format` passes
- [ ] Manual verification with CLI succeeds

---

## Phase 2: Fix Spurious Warnings

### Problem Summary

Properties like `param_name` and `data_type` on `parameter` node are literal-only (defined in `get_text_properties()` but not in `node_type.parameters`). The warning check only looks at `valid_params` (wirable parameters), causing spurious warnings.

### Files to Modify

| File | Lines | Change |
|------|-------|--------|
| `rust/src/structure_designer/text_format/network_editor.rs` | 462-502 | Check both `valid_params` and text properties |

### Implementation Steps

#### Step 2.1: Gather text property names

After getting `valid_params` and `node_type_name` (around line 480), add:

```rust
// Get text property names (for literal-only properties that aren't in parameters)
let text_prop_names: std::collections::HashSet<String> = self
    .network
    .nodes
    .get(&node_id)
    .map(|node| {
        node.data
            .get_text_properties()
            .iter()
            .map(|(name, _)| name.clone())
            .collect()
    })
    .unwrap_or_default();
```

#### Step 2.2: Update warning condition

Change the warning check (currently checking only `valid_params`) to:

```rust
// Warn about unknown properties (only for values we're actually applying)
// A property is "known" if it's either a wirable parameter OR a text-only property
if !valid_params.is_empty()
    && !valid_params.contains(prop_name)
    && !text_prop_names.contains(prop_name)
{
    self.result.add_warning(format!(
        "Unknown property '{}' on node type '{}'",
        prop_name, node_type_name
    ));
}
```

### Testing

Add test to `rust/tests/structure_designer/text_format_test.rs`:

```rust
#[test]
fn test_no_spurious_warnings_for_literal_only_properties() {
    let registry = create_test_registry();
    let mut network = create_test_network();

    let result = edit_network(&mut network, &registry, r#"
        p = parameter { param_name: "size", data_type: Float }
    "#, true);

    assert!(result.success, "Edit should succeed: {:?}", result.errors);

    // Should have no warnings about param_name or data_type
    for warning in &result.warnings {
        assert!(!warning.contains("param_name"),
            "Should not warn about param_name: {}", warning);
        assert!(!warning.contains("data_type"),
            "Should not warn about data_type: {}", warning);
    }
}
```

### Verification Command

```bash
# After fix, this should have no warnings:
echo "p = parameter { param_name: \"size\", data_type: Float }" | atomcad-cli edit --replace
# Expected: No warnings about 'param_name' or 'data_type'
```

### Done Criteria

- [ ] No warnings for valid literal-only properties
- [ ] Warnings still appear for truly unknown properties
- [ ] New test passes
- [ ] `cargo test text_format` passes

---

## Phase 3: Improve `describe` for Dynamic Nodes

### Problem Summary

Two related issues with the `describe` command output for dynamic nodes like `expr` and `parameter`:

1. **Issue #3:** `DataType::None` displays as "None" which is confusing for dynamic types
2. **Issue #4:** Dynamic input pins (generated by `calculate_custom_node_type()`) aren't shown

### Files to Modify

| File | Lines | Change |
|------|-------|--------|
| `rust/src/structure_designer/text_format/node_type_introspection.rs` | 80-275 | Add dynamic type formatting and show calculated pins |

### Implementation Steps

#### Step 3.1: Add helper function for formatting dynamic types

Add near the top of the file (after imports, around line 74):

```rust
/// Formats a DataType for display, showing "dynamic" instead of "None" for dynamic types.
fn format_data_type_for_display(dt: &DataType) -> String {
    match dt {
        DataType::None => "dynamic".to_string(),
        DataType::Array(inner) if matches!(**inner, DataType::None) => "[dynamic]".to_string(),
        other => other.to_string(),
    }
}
```

#### Step 3.2: Update output type display

Change line 272 from:
```rust
writeln!(output, "Output: {}", node_type.output_type.to_string()).unwrap();
```

To:
```rust
writeln!(output, "Output: {}", format_data_type_for_display(&node_type.output_type)).unwrap();
```

#### Step 3.3: Add dynamic pins section for nodes with custom node types

After processing the regular inputs (after line 266), add logic to show dynamic pins:

```rust
// For nodes with dynamic pins (expr, parameter), show the calculated custom node type
let custom_node_type = default_data.calculate_custom_node_type(&node_type);
if let Some(ref custom_type) = custom_node_type {
    // Check if the custom type differs from the base type
    let has_dynamic_params = custom_type.parameters != node_type.parameters;
    let has_dynamic_output = custom_type.output_type != node_type.output_type;

    if has_dynamic_params || has_dynamic_output {
        writeln!(output).unwrap();
        writeln!(output, "Dynamic Configuration (default instance):").unwrap();

        if has_dynamic_params && !custom_type.parameters.is_empty() {
            writeln!(output, "  Dynamic Inputs:").unwrap();
            for param in &custom_type.parameters {
                writeln!(
                    output,
                    "    {} : {}  [required]",
                    param.name,
                    format_data_type_for_display(&param.data_type)
                ).unwrap();
            }
        }

        if has_dynamic_output {
            writeln!(
                output,
                "  Dynamic Output: {}",
                format_data_type_for_display(&custom_type.output_type)
            ).unwrap();
        }
    }
}
```

#### Step 3.4: Update type display in inputs section

Also update line 211 to use the new helper:
```rust
let type_str = format_data_type_for_display(&param.data_type);
```

And line 254:
```rust
let type_str = format_data_type_for_display(&value.inferred_data_type());
```

### Expected Output Improvements

**Before:**
```
Node: expr
Category: MathAndProgramming
Description: ...

Inputs:
  expression : String  [default: "x", literal-only]
  parameters : [None]  [default: [...], literal-only]

Output: None
```

**After:**
```
Node: expr
Category: MathAndProgramming
Description: ...

Inputs:
  expression : String  [default: "x", literal-only]
  parameters : [dynamic]  [default: [...], literal-only]

Output: dynamic

Dynamic Configuration (default instance):
  Dynamic Inputs:
    x : Int  [required]
  Dynamic Output: Int
```

### Testing

Add tests to `rust/tests/structure_designer/text_format_test.rs`:

```rust
#[test]
fn test_describe_expr_shows_dynamic() {
    let registry = create_test_registry();
    let result = describe_node_type("expr", &registry);

    // Should show "dynamic" instead of "None"
    assert!(result.contains("dynamic"),
        "Should show 'dynamic' for expr output type, got:\n{}", result);
    assert!(!result.contains("Output: None"),
        "Should NOT show 'Output: None', got:\n{}", result);
}

#[test]
fn test_describe_expr_shows_dynamic_pins() {
    let registry = create_test_registry();
    let result = describe_node_type("expr", &registry);

    // Should show dynamic configuration section
    assert!(result.contains("Dynamic Configuration") || result.contains("Dynamic Inputs"),
        "Should show dynamic inputs section for expr, got:\n{}", result);
}

#[test]
fn test_describe_parameter_shows_dynamic() {
    let registry = create_test_registry();
    let result = describe_node_type("parameter", &registry);

    // parameter node also has dynamic output type
    assert!(result.contains("dynamic") || result.contains("Dynamic"),
        "Should show dynamic information for parameter node, got:\n{}", result);
}
```

### Verification Command

```bash
# After fix:
atomcad-cli describe expr
# Expected output should show:
#   - "dynamic" instead of "None"
#   - Dynamic Inputs section with "x : Int"
#   - Dynamic Output: Int
```

### Done Criteria

- [ ] `DataType::None` displays as "dynamic"
- [ ] `DataType::Array(Box<None>)` displays as "[dynamic]"
- [ ] Dynamic nodes show calculated pins section
- [ ] New tests pass
- [ ] `cargo test text_format` passes
- [ ] Manual verification with `atomcad-cli describe expr` shows improved output

---

## Implementation Order Recommendation

1. **Start with Phase 1** - It's the critical blocker for Mode 1 functionality
2. **Phase 2 can be done in parallel** - Independent and quick
3. **Phase 3 after Phase 1** - Benefits from understanding the codebase better

## Running All Tests

```bash
cd rust

# Run all text format tests
cargo test text_format

# Run specific test file
cargo test --test structure_designer text_format_test

# Run with output
cargo test text_format -- --nocapture
```

## Commit Strategy

Recommend one commit per phase:

1. `fix: handle array literal properties in text format editor`
2. `fix: suppress spurious warnings for literal-only properties`
3. `feat: improve describe output for dynamic node types`

---

## Cross-References

- Issue analysis: [text_format_issues.md](text_format_issues.md)
- Testing conventions: [../rust/AGENTS.md](../rust/AGENTS.md)
- Text format syntax: [../.claude/skills/atomcad/skill.md](../.claude/skills/atomcad/skill.md)
