# Text Format - Agent Instructions

Human-readable text format for node networks. Primary purpose: enable AI assistants to read and edit node networks programmatically.

## Files

| File | Purpose |
|------|---------|
| `parser.rs` | Lexer + parser: text → `Statement` AST |
| `serializer.rs` | `TextValue` → string representation |
| `network_serializer.rs` | `NodeNetwork` → text format (topologically sorted) |
| `network_editor.rs` | Applies parsed statements to a `NodeNetwork` |
| `auto_layout.rs` | Positions newly created nodes intelligently |
| `text_value.rs` | `TextValue` enum: typed property values |
| `node_type_introspection.rs` | Generates human-readable node type descriptions |

## Text Format Syntax

```
-- Comment
my_sphere = Sphere(radius: 5.0)
my_box = Cuboid(size: (10, 10, 10))
result = Union(a: my_sphere, b: my_box)
output result
```

- **Assignment:** `name = NodeType(prop: value, input: other_node)`
- **Output:** `output node_name` (sets the network's return node)
- **Delete:** `delete node_name`
- **Description:** `description "Network description text"`
- **Summary:** `summary "One-line summary"`
- **Visibility:** `name = NodeType(..., visible: true)`
- **Function refs:** `func: @network_name` (reference another network)
- **Arrays:** `values: [1, 2, 3]`
- **Vectors:** `pos: (1.0, 2.0, 3.0)`
- **Strings:** `name: "hello"` or `name: '''multi-line'''`

## NetworkEditor (network_editor.rs)

Applies edits from parsed text to a `NodeNetwork`. Two-pass approach:
1. **Create pass:** Create/update nodes with literal properties
2. **Wire pass:** Connect node-to-node references as wires

Supports two modes:
- **Replace mode:** Clears network first, then creates from scratch
- **Incremental mode:** Merges new statements with existing nodes

Returns `EditResult` with success/failure/warning counts.

## NetworkSerializer (network_serializer.rs)

Converts a `NodeNetwork` back to text format:
- Topological sort ensures dependencies appear before dependents
- Handles multi-input pins, function references, visibility
- Cycle detection with error reporting

## Auto-Layout (auto_layout.rs)

Calculates positions for newly created nodes:
- Strategy 1: Place right of connected input nodes at average Y
- Strategy 2: Place in empty space for unconnected nodes
- Overlap avoidance with existing nodes

## TextValue (text_value.rs)

Typed value representation for properties:
```
Bool, Int, Float, String, Vec2, Vec3, IVec2, IVec3,
DataType, Array(Vec<TextValue>), Object(HashMap)
```

Supports type coercion (Int→Float, IVec→Vec) and conversion to `NetworkResult`.

## Testing

Tests in `rust/tests/structure_designer/text_format_test.rs`.
