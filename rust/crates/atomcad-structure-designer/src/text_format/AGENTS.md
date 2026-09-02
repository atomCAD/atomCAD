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

Applies edits from parsed text to a `NodeNetwork`. Four passes, and the
**borrow discipline is what dictates them**: `Node.zone` is an
`Arc<NodeNetwork>` mutated through `zone_mut()` → `Arc::make_mut`, so a body
borrow cannot be held while the parent's name map is read — which is exactly
what resolving a `^capture` needs.

0. **Snapshot pass:** record `(name path) → position` over the whole network
   including bodies, **before** `clear_network` in replace mode.
1. **Create pass:** create/update nodes with literal properties, recursing
   into `body { … }` blocks.
2. **Wire pass:** connect references as wires, then comment anchors, then
   visibility.
3. **Deferred pass:** the `delete` / `output` statements, in source order.

No borrow is ever held across a scope boundary: `scope_net` / `scope_net_mut`
re-walk the scope path from the root each time (`O(depth)`, and depth is 1 or 2
in practice). Everything that used to be flat — the name maps, pending
connections, visible nodes, anchors — is keyed by **scope path**, the chain of
zone-owning node ids down to the body.

Supports two modes:
- **Replace mode:** Clears network first, then creates from scratch
- **Incremental mode:** Merges new statements with existing nodes

The in-app *Text* tab always uses **replace** mode
(`api::…::apply_text_to_active_network`), so incremental mode is reached only
through the AI-assistant HTTP `/edit?replace=false` and `atomcad-cli edit` —
where it is the **default**. A bug that only shows in incremental mode is
therefore invisible in the app and hits every AI edit.

Returns `EditResult` with success/failure/warning counts. Its node lists carry
**full paths** (`m1/a`), not bare names — with bodies in play `m1/a` and `m2/a`
are different nodes.

`EditResult.success` means **parsed, applied *and* validates**: `ai_edit_network`
folds `validate_network`'s verdict in (blocking → `errors`, non-blocking →
`warnings`), which is the AI's only signal that an edit broke something. The
gates that are about *what the editor did* — auto-layout, the dirty flag — read
a separate `edit_applied` flag captured before that fold, so a valid-but-flagged
edit still lays out and still marks the project dirty.

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
- Projects HOF/closure zone bodies as nested `body { … }` blocks (see below)

## Zone bodies (`body { … }`)

`doc/design_hof_body_text_format.md`. The serializer, the parser and the editor
are all scope-aware: `query` projects bodies and `edit` accepts them, either as
a whole block or one node at a time through a path (`m1/x = …`).

A zone-owning node's statement becomes multi-line, with the body block as its
last property-position item:

```
m1 = map {
  xs: r,
  body {
    d = mul { a: $element, b: ^scale }
    output d
  }
}
```

Four spellings, and nothing else. With `k` = the number of leading `^`:
**`k` carets → `NodeOutput` at depth `k`; a `$` prefix → `ZoneInput` at depth
`k + 1`.** The `+ 1` is a real asymmetry in the data model, not a quirk of the
format: a `NodeOutput` depth counts *networks* (`0` = this body) while a
`ZoneInput` depth counts *owning-HOF body frames* (`1` = this body's own
owner) — the same arithmetic `network_evaluator.rs`'s `ZoneInput` arm does.
`format_wire_source` is the single implementation; don't re-derive it.

Three things that are easy to get wrong here:

- **Zone pin names are not uniform.** Only `map` / `zip_with` call the output
  `result` (`filter` → `keep`, `foreach` → `out`, `fold` → `acc`/`new_acc`,
  `zip_with` → `element1…N`, `closure` → its own `param_names`). Read them off
  the resolved `NodeType::zone_input_pins`, never from a hand-written table.
- **`output` inside a block writes the *parent HOF node's*
  `zone_output_arguments`**, not the body network's `return_node_id`. Same
  keyword as the network-level `output`; only the position distinguishes them.
- **An empty body emits no block at all.** That is the shape of an HOF driven
  through its `f:` pin, and an omitted `body` means "untouched" — which for an
  already-empty body is the same state, so the round-trip stays exact.

Consumers that assumed "one statement = one line" must brace-match now.

### Writing one: what a block assigns

A mentioned `body { … }` block assigns the **whole** body — the same rule as
any other property — and it is total over the parent's `zone_output_arguments`
too: a block with **no** `output` statement *clears* the zone-output wire. That
is the only reading under which `query` → `edit --replace` is exact, and it is
easy to "fix" into a bug by making an absent `output` mean "leave it".

Two things the editor must keep doing:

- **Identity is name-keyed, per scope, and carries `position`.** A rebuilt body
  node matched by name keeps its node id and its position; an unmatched name is
  new. Positions are **not in the text format at all** — the serializer emits
  none and `create_node` synthesizes one — so the only way one survives is the
  Pass 0 snapshot, which must be taken *ahead of* `clear_network`. Without it,
  every `--replace` scrambles the layout of everything inside every body.
  That snapshot is the **public** `snapshot_node_positions`, not a private
  method: `ai_edit_log` measures what layout did to a drawing against exactly
  this identity match (`doc/design_ai_edit_history.md` D9), and two path-keyed
  walks would drift. Keep it shared, and keep the key a `NamePath` — an id
  matches nothing across a `--replace`, and a bare name collides across scopes.
- **A statement's properties are applied before its body block.** A `closure`'s
  zone-input *names* are its own `params:`, which the same statement may be
  setting — so `$x` cannot bind until they are in place. This is structural
  rather than remembered: the parser keeps the body out of `properties`, and
  `apply_body` runs after `create_node` / `update_node` (which is also what
  makes the owner's resolved `NodeType::zone_input_pins` current by then).
  Do not "tidy" the body into the property list.

Zone init is **eager**: `ensure_zone` calls `ensure_zone_init` itself rather
than leaving it to validation, which runs long after the editor returns and so
would give the body statements nowhere to land.

### Path addressing: `m1/x = …`

The second edit granularity. **The prefix selects the scope, the last segment
names the node in it** — one rule, applied to all three statement forms:

```
m1/d = mul { a: $element, b: ^scale }   # update (or create) one body node
output m1/d                              # re-point m1's zone-output wire
delete m1/e                              # remove one body node
outer/inner/n = add { a: $element, b: ^^base }
```

The separator is `/`, never `.`: `.` already means pin access in value
position, so `output m1.d` would be ambiguous. Paths split on the `/` *token*,
so a backtick-quoted segment containing a slash (`` m1/`a/b` ``) is still one
segment.

Three things follow, and they are the whole semantics:

- **A block replaces, a path merges.** `m1 = map { body { … } }` is total over
  the body *and* over the zone-output wire; `m1/x = …` touches `x` and leaves
  every sibling's id and position alone. Ordering within one script is
  significant and needs no special rule — the block wipes, a later path
  statement merges into the result.
- **Everything inside a path statement is relative to the scope it lands on.**
  Bare names resolve in the addressed body, `$…` are *its* zone inputs and
  `^…` walks outward from *it*, so `m1/x = …` and the same statement written
  inside `m1`'s block mean exactly the same thing. `resolve_path_scope` walks
  the prefix and then hands the ordinary per-scope machinery a deeper scope;
  nothing downstream knows a path was involved.
- **A path never creates the HOF it addresses**, and never guesses: a segment
  that names no node, or names one that owns no body, is an error rather than a
  silent no-op. `output m1/x` inside a body block is *not* that block's own
  `output` — the block's is the unqualified one, and a path-addressed
  assignment mentions its **first segment**, which is what stops the enclosing
  block from deleting the body the next statement reaches into.

The serializer still emits only the block form (D9): paths are input-only
sugar, so `query` output stays single-valued and one node keeps one statement
shape.

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
