# Research: Auto-Inference of expr Node Input Parameters

## Problem

When creating an `expr` node via CLI/text format, users must manually specify the `parameters` array if they need inputs beyond the default `x`:

```
# This fails - 'y' parameter doesn't exist
result = expr { expression: "x + y", x: a, y: b }
# Warning: Parameter 'y' not found on node type 'expr'

# This works but is verbose
result = expr {
  expression: "x + y",
  parameters: [{ name: "x", data_type: Int }, { name: "y", data_type: Int }],
  x: a,
  y: b
}
```

This creates friction for AI assistants and users who expect wiring `y: b` to "just work".

## Proposed Solution

Auto-infer the `parameters` array from:
1. The wired connections (e.g., `x: a, y: b` implies parameters x and y)
2. The expression text (parse to find referenced variables)
3. The data types of connected source nodes

## Open Questions

1. **Where should inference happen?**
   - In `set_text_properties()` when parameters aren't explicitly provided?
   - In the text format editor (`network_editor.rs`) before wiring?
   - At connection time when a parameter doesn't exist?

2. **How to determine data types?**
   - Infer from connected source node's output type?
   - Default to a generic type (e.g., Float)?
   - Parse expression to infer from usage context?

3. **What about partial specification?**
   - User provides some parameters explicitly, others should be inferred?

4. **Backwards compatibility?**
   - Should this be opt-in or default behavior?
   - How to handle existing networks that might behave differently?

5. **Implicit type conversions complicate inference**

   The node network supports implicit conversions (see `rust/src/structure_designer/data_type.rs`):
   - `Int` ↔ `Float`
   - `IVec2` ↔ `Vec2`, `IVec3` ↔ `Vec3`
   - `T` → `[T]` (scalar to single-element array)

   If source node outputs `Int` but expression uses it as `Float` (e.g., `x / 2.0`), what should the inferred parameter type be?
   - Use source type (`Int`) and rely on expression-level type promotion?
   - Analyze expression to determine required type?
   - This interacts with expr's own type promotion rules (integers promote to floats when mixed).

## Relevant Code Locations

- `rust/src/structure_designer/nodes/expr.rs` - ExprData and set_text_properties
- `rust/src/structure_designer/text_format/network_editor.rs` - wire_connection, get_param_index
- `rust/src/expr/parser.rs` - expression parsing (could extract variable names)

## Success Criteria

The following should work without explicit `parameters`:
```
a = int { value: 5 }
b = int { value: 3 }
result = expr { expression: "a + b", a: a, b: b }
```
