# Issue: expr Node Evaluation Fails with "Expression not parsed"

## Summary

The `expr` node fails to evaluate via the CLI `evaluate` command, returning "Error: Expression not parsed" even for simple, valid expressions.

## Severity

**High** - The expr node is a core math/programming feature, and being unable to evaluate its output makes debugging and verification difficult.

## Steps to Reproduce

```bash
# 1. Start atomCAD (ensure the application is running)

# 2. Create a simple expr node with a basic expression
cd c:/machine_phase_systems/flutter_cad
./atomcad-cli edit --replace --code="x = int { value: 10 }"
./atomcad-cli edit --code="simple_expr = expr { expression: \"x * 2\", x: x }"

# 3. Verify the node was created correctly
./atomcad-cli query
# Expected output shows: simple_expr = expr { x: x, expression: "x * 2", parameters: [{ name: "x", data_type: Int }] }

# 4. Try to evaluate the expr node
./atomcad-cli evaluate simple_expr
# Output: Error

./atomcad-cli evaluate simple_expr --verbose
# Output: Error: Expression not parsed
```

## Expected Behavior

The evaluate command should return `20` (the result of `10 * 2`).

## Actual Behavior

Returns "Error: Expression not parsed" even though:
1. The expression `"x * 2"` is valid syntax according to `describe expr`
2. The node is created without errors
3. The input `x` is properly wired (as shown in the JSON response)

## Additional Test Cases

```bash
# Even simpler expression - still fails
./atomcad-cli edit --code="expr2 = expr { expression: \"x + 1\", x: x }"
./atomcad-cli evaluate expr2 --verbose
# Output: Error: Expression not parsed

# Expression with two variables (note: y parameter warning)
./atomcad-cli edit --code="y = int { value: 3 }"
./atomcad-cli edit --code="sum1 = expr { expression: \"x + y\", x: x, y: y }"
# Warning: Connection warning for sum1.y: Parameter 'y' not found on node type 'expr'
# This suggests dynamic pin creation may not be working via CLI
```

## Relevant Files to Investigate

Based on the project structure, likely locations:
- `rust/src/structure_designer/nodes/` - Node implementations
- `rust/src/api/` - API endpoints for evaluate
- Expression parsing logic (possibly in `rust/src/expr/`)

## Notes

1. The `describe expr` documentation shows the node supports dynamic input pins configured via a `parameters` property
2. When queried, the node shows `parameters: [{ name: "x", data_type: Int }]` suggesting only one dynamic pin is created by default
3. The issue might be:
   - Expression parsing happening at evaluation time rather than node creation
   - Missing dynamic pin configuration through CLI
   - Serialization/deserialization issue with the expression string

## Environment

- Platform: Windows
- CLI: atomcad-cli via bash wrapper
- Server port: 19847 (default)
