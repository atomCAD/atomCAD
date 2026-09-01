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
- **Matrices:** `m: ((1, 0, 0), (0, 1, 0), (0, 0, 1))` — nested tuples, row-major. Parsed as `IMat3` or `Mat3` based on the target pin's declared type.
- **Strings:** `name: "hello"` or `name: '''multi-line'''`
- **Multi-output pin refs:** `input: atom_edit.diff` (selects pin by name). Unqualified `input: atom_edit` defaults to pin 0. Serializer emits `.pinname` only for pin index > 0.
- **Comment anchors:** `on: mybox` (node) or `on: mybox -> union.a` (wire, `->` is a lexer token), or an array mixing both. See `doc/node_network_text_format.md`.

## Network-level properties (`visible`, `on`)

Two properties inside a node's braces are **not** `NodeData` state and so cannot
go through `get_text_properties` / `set_text_properties` — the latter receives
only a `HashMap<String, TextValue>` and can resolve neither a node id nor a node
name. `visible` lives in `NodeNetwork.displayed_nodes`; `on` (comment anchors,
`doc/design_wire_annotations.md`) is a list of node ids. Both follow the same
three-site shape, and a third such property must too:

1. **`network_serializer.rs`** — an extra pass in `serialize_node` that emits the
   property from network state, turning ids back into names.
2. **`network_editor.rs::apply_literal_properties`** — skip the name, so it is
   neither fed to `set_text_properties` nor reported as an unknown property.
3. **`network_editor.rs::collect_connections`** — intercept it *before* the
   generic reference handling, which would otherwise try to wire it to a
   parameter of that name. Park it in a pending list resolved in a later pass,
   once every node exists and names map to ids.

Anchors additionally resolve **after** `wire_pending_connections`, not with the
`visible` pass: a wire anchor names a wire, and that is the pass which creates
it.

## NetworkEditor (network_editor.rs)

Applies edits from parsed text to a `NodeNetwork`. Two-pass approach:
1. **Create pass:** Create/update nodes with literal properties
2. **Wire pass:** Connect node-to-node references as wires

Supports two modes:
- **Replace mode:** Clears network first, then creates from scratch
- **Incremental mode:** Merges new statements with existing nodes

The in-app *Text* tab always uses **replace** mode
(`api::…::apply_text_to_active_network`), so incremental mode is reached only
through the AI-assistant HTTP `/edit?replace=false` and `atomcad-cli edit` —
where it is the **default**. A bug that only shows in incremental mode is
therefore invisible in the app and hits every AI edit.

Returns `EditResult` with success/failure/warning counts.

### A mentioned property assigns the pin's whole wire set

This is the invariant that makes incremental mode able to *remove* a wire, and
it is easy to break by accident, because the natural implementation of "wire
this up" only clears on the way to writing something.

`wire_connection` clears the destination `Argument` **unconditionally** —
including array pins, which is what lets `shapes: [a, b, c]` shrink to
`shapes: [a]` — and `collect_connections` queues a `PendingConnection` even when
the property named **no** source, so a literal (or `[]`) reaches that clear. Do
not "optimize" the source-free queueing away: without it a literal on a wired
pin lands in the node's data, the wire keeps winning at evaluation, and
`network_serializer` suppresses the stored value on a wired pin — a silent
no-op reported as `success: true`, which is how this shipped for a long time.

`property_disconnects_pin` holds the two exemptions, and both matter: a
**wire-only** pin's literal is rejected with a warning, so clearing its wire
would leave it with neither a wire nor a value; and a name that is not a
parameter at all (a text-only property like `polygon.vertices`) has no pin to
clear. A property omitted from the statement is untouched — that is what
"incremental" means, and it is why the *only* pin an edit disconnects is one it
names.

Tests: `text_format_test.rs::literal_disconnect_tests`. The user-facing
contract is in `doc/node_network_text_format.md` (§Edit semantics) and
`.claude/skills/atomcad/references/text-format.md`.

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
Bool, Int, Float, String, Vec2, Vec3, IVec2, IVec3, IMat3, Mat3,
DataType, Array(Vec<TextValue>), Object(HashMap)
```

Matrix variants store row-major `[[i32; 3]; 3]` / `[[f64; 3]; 3]`; conversion to `NetworkResult::Mat3(DMat3)` transposes at the boundary (DMat3 is column-major internally).

Supports type coercion (Int→Float, IVec→Vec, IMat3→Mat3) and conversion to `NetworkResult`.

## Testing

Tests in `crates/atomcad-structure-designer/tests/structure_designer/text_format_test.rs`.
