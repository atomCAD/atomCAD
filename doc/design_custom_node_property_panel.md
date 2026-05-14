# Design: Auto-Generated Property Panel for Custom Nodes

## Problem Statement

Built-in nodes have hand-written property-panel editors (`float_editor.dart`,
`vec3_editor.dart`, …) registered in `node_data_widget.dart`. Custom nodes —
node types implemented by user-defined node networks — have **no editor at
all**: when one is selected, `node_data_widget.dart` falls through its node-type
`switch` to the `default:` arm and shows *"No editor available for
&lt;type&gt;"*.

We want custom nodes to get an **automatically generated** property panel: one
input field per input pin whose data type is a simple, editable scalar/vector/
matrix type. The user can set a value inline; a value wired into the pin
overrides the inline value during evaluation.

## Key Finding: The Backend Already Exists

The storage + evaluation half of this feature is **already implemented** (see
`doc/design/custom_node_literal_params.md`, which has shipped):

| Piece | Location | Status |
|---|---|---|
| Per-node literal storage | `CustomNodeData { literal_values: HashMap<String, TextValue> }` in `node_data.rs` | ✅ done |
| Eval priority: **wired pin > stored literal > default pin** | `nodes/parameter.rs::eval` | ✅ done |
| Value coercion | `TextValue::to_network_result` in `text_format/text_value.rs` | ✅ done |
| Serialization round-trip | `generic_node_data_saver/loader::<CustomNodeData>` | ✅ done |
| Text-format literal syntax (`octahedron { size: 8 }`) | `text_format/network_editor.rs` | ✅ done |

`CustomNodeData` already implements `get_text_properties` / `set_text_properties`
over `literal_values`, and every custom node instance already carries a
`CustomNodeData` as its `node.data`.

**Therefore this feature is purely a UI + thin-API addition.** No changes to the
evaluator, to `parameter.rs`, or to serialization are required.

## Scope

### In scope

- A small FFI surface to **query** the editable parameters of a custom node
  (name, type, stored value, resolved default, wired-or-not) and to **write**
  /**clear** a stored literal. The query resolves each parameter's default by
  evaluating the subnetwork in isolation, so it runs through
  `with_mut_cad_instance` — logically a read, but not side-effect-free (see
  `resolve_parameter_default`).
- A new Flutter widget `CustomNodeEditor` that renders one row per editable
  parameter, wired into `node_data_widget.dart`'s `default:` arm.
- Reuse of the existing primitive input widgets (`FloatInput`, `IntInput`,
  `Vec3Input`, the `IntMatrixCell`/`FloatMatrixCell` grid, …).

### Editable ("simple") data types

`bool`, `int`, `float`, `string`, `ivec2`, `ivec3`, `vec2`, `vec3`, `imat3`,
`mat3`. Every other pin type (`Blueprint`, `Crystal`, `Molecule`, `Structure`,
`Array[..]`, `Iter[..]`, `Record(..)`, `Function(..)`, abstract supertypes, …)
is **omitted** from the panel — it stays wire-only.

### Out of scope

- Renaming a parameter inside the subnetwork orphans its `literal_values` entry
  (keyed by name, not by `Parameter.id`). Orphan entries are inert — eval
  ignores any key without a matching current parameter, and the getter never
  surfaces them — but they linger in the `.cnnd` file. Acceptable for v1; a
  future cleanup pass in `repair_node_network` could prune them.
- Editing literals for the *abstract*/complex types listed above.

## Architecture

```
┌─ Flutter ───────────────────────────────────────────────┐
│ node_data_widget.dart (default: arm)                    │
│   └─ CustomNodeEditor (NEW)                             │
│        └─ one row per APICustomNodeParam                │
│             reuses FloatInput / IntInput / Vec3Input /  │
│             matrix cells / bool toggle / text field     │
│   model: getCustomNodeParams / setCustomNodeLiteral /   │
│          clearCustomNodeLiteral                         │
├─ FFI (structure_designer_api.rs) ───────────────────────┤
│   get_custom_node_params(node_id)                       │
│   set_custom_node_literal(node_id, name, value)         │
│   clear_custom_node_literal(node_id, name)              │
├─ Rust core (already exists) ────────────────────────────┤
│   CustomNodeData.literal_values  +  parameter.rs eval   │
└─────────────────────────────────────────────────────────┘
```

## FFI Surface

### New API types (`api/structure_designer/structure_designer_api_types.rs`)

```rust
/// The editable subset of `TextValue` that the custom-node panel can render.
/// Mirrors the "simple" data types. FRB data-carrying enum — same shape as the
/// existing `APIMeasurement` enum, so this pattern is already proven here.
///
/// Named for the `CustomNodeData.literal_values` map it is read from / written
/// to — deliberately *not* `APITextValue`, since the "text" in the core
/// `TextValue` type refers to the node-network text format, which is unrelated
/// to this panel.
pub enum APILiteralValue {
    Bool(bool),
    Int(i32),
    Float(f64),
    Str(String),
    IVec2(APIIVec2),
    IVec3(APIIVec3),
    Vec2(APIVec2),
    Vec3(APIVec3),
    /// Row-major 3x3, matching `TextValue::IMat3`.
    IMat3(Vec<Vec<i32>>),
    /// Row-major 3x3, matching `TextValue::Mat3`.
    Mat3(Vec<Vec<f64>>),
}

/// One editable parameter (input pin) of a custom node.
pub struct APICustomNodeParam {
    pub name: String,
    pub data_type: APISimpleParamType,
    /// The literal currently stored in `CustomNodeData.literal_values`, if any
    /// AND it still matches `data_type`. `None` ⇒ render the placeholder.
    pub stored_value: Option<APILiteralValue>,
    /// The value the parameter node's `default` input pin resolves to, used as
    /// the field placeholder. `None` when the default pin is unconnected or
    /// evaluation fails / yields a non-simple type.
    pub default_value: Option<APILiteralValue>,
    /// True when the parent pin has a wire connected. When true the row renders
    /// disabled (see "Disable on wired input" pattern).
    pub is_wired: bool,
}

/// Dedicated enum so the Flutter widget switches directly without parsing pin
/// type strings or depending on `APIDataTypeBase`'s coverage.
pub enum APISimpleParamType {
    Bool, Int, Float, Str, IVec2, IVec3, Vec2, Vec3, IMat3, Mat3,
}
```

### New API functions (`api/structure_designer/structure_designer_api.rs`)

```rust
/// Returns `None` if `node_id` is not a custom node (its `node_type_name` is
/// not in `registry.node_networks`). Returns `Some(vec)` — possibly empty — for
/// a custom node, listing only its simple-typed parameters, in pin order.
///
/// Runs through `with_mut_cad_instance`: resolving each parameter's default
/// (`resolve_parameter_default`) evaluates the subnetwork and needs `&mut self`.
#[flutter_rust_bridge::frb(sync)]
pub fn get_custom_node_params(node_id: u64) -> Option<Vec<APICustomNodeParam>>;

/// Inserts/updates `literal_values[param_name]`. Goes through
/// `set_node_network_data`, so it gets the existing `SetNodeData` undo command
/// and `refresh_structure_designer_auto` for free.
#[flutter_rust_bridge::frb(sync)]
pub fn set_custom_node_literal(node_id: u64, param_name: String, value: APILiteralValue);

/// Removes `literal_values[param_name]`. Same `set_node_network_data` path.
#[flutter_rust_bridge::frb(sync)]
pub fn clear_custom_node_literal(node_id: u64, param_name: String);
```

#### `get_custom_node_params` algorithm

Runs inside `with_mut_cad_instance` — step 3 calls `resolve_parameter_default`,
which is `&mut self` (see "Resolving the placeholder" below).

1. Resolve the active network's node `node_id`. If `node.node_type_name` is not
   a key in `registry.node_networks` → return `None`.
2. Get the custom node's parameter list — `node.custom_node_type.parameters`
   (the cache; falls back to looking up the network's `NodeType` if the cache
   is empty). `network_validator` rebuilds this `Vec<Parameter>` sorted by each
   parameter node's `ParameterData.sort_order`, then `node_id` as a tiebreaker
   — note `sort_order` lives on `ParameterData`, not `Parameter` — and in the
   same pass sets each parameter node's `param_index` to its position in this
   Vec.
3. For each `Parameter { name, data_type, .. }` at index `i`:
   - Map `data_type` → `APISimpleParamType`. If it is not one of the simple
     types, **skip** this parameter.
   - `is_wired = !node.arguments[i].is_empty()`. Safe because the step-2
     `network_validator` invariant keeps `parameters[i]`, the parameter node's
     `param_index`, and the call-site `arguments[i]` in lockstep — the same `i`
     that `parameter.rs::eval` uses.
   - `stored_value`: `custom_data.literal_values.get(name)`, converted to
     `APILiteralValue`. If the stored `TextValue` does not match the current
     `data_type` (parameter was retyped in the subnetwork), treat as `None`.
   - `default_value`: resolve the parameter's `default` pin — see below.
4. Return `Some(vec)`.

#### Resolving the placeholder (`default_value`)

The placeholder is the value the parameter node's `default` input pin (argument
index 0) evaluates to *inside the subnetwork*. `parameter.rs::eval` already has
the right behaviour: when evaluated with a network stack of length &lt; 2
("in isolation") it returns `eval_default(...)` — exactly the `default` pin's
value.

Add a core helper (in `structure_designer.rs`, near the other evaluation
helpers):

```rust
/// Best-effort: evaluate the `default` input pin of the parameter node named
/// `param_name` inside `subnetwork_name`, in isolation. Returns `None` on any
/// error, on a missing/unconnected default pin, or on a non-simple result type.
///
/// Takes `&mut self`: evaluation goes through `with_eval_context`, which
/// mutably borrows the evaluator and drains the print buffer. This is logically
/// a read, but not side-effect-free — every caller up the chain
/// (`get_custom_node_params`, its FFI wrapper) needs `&mut self` too.
fn resolve_parameter_default(&mut self, subnetwork_name: &str, param_name: &str)
    -> Option<NetworkResult>;
```

It finds the parameter node whose `ParameterData.param_name == param_name`,
builds a single-element network stack for the subnetwork, and evaluates that
node via the standard `NetworkEvaluator` (use the existing `with_eval_context`
construction site so we don't open a new `NetworkEvaluationContext` site —
`execute = false`). Convert the resulting `NetworkResult` to `APILiteralValue`;
anything that isn't a simple type → `None`.

Cost note: default pins are almost always a single literal node, so this is
cheap. In the pathological case (a default pin wired through a heavy subgraph)
it costs one evaluation per panel build — the same work the user would pay to
open that subnetwork as the active network. Acceptable; revisit with caching
only if a hot path is established.

#### `set_custom_node_literal` / `clear_custom_node_literal`

Both follow the existing primitive-setter pattern (`set_float_data` &c.):

```rust
with_mut_cad_instance(|cad| {
    // clone current CustomNodeData, mutate the HashMap, box it back
    let mut data: CustomNodeData = /* downcast-clone of node.data */;
    data.literal_values.insert(param_name, value.into());   // or .remove(&param_name)
    cad.structure_designer.set_node_network_data(node_id, Box::new(data));
    refresh_structure_designer_auto(cad);
});
```

`set_node_network_data` already snapshots before/after and pushes a
`SetNodeData` undo command — **no new undo command type is needed.** A literal
edit is undone/redone exactly like a `float` node's value edit.

## Flutter UI

### Wiring into the router (`node_data/node_data_widget.dart`)

Custom node type names are dynamic, so they cannot be `case` labels. Change the
`default:` arm of `_buildNodeEditor`:

```dart
default:
  final params = getCustomNodeParams(nodeId: selectedNode.id);
  if (params == null) {
    return Center(child: Text('No editor available for ${selectedNode.nodeTypeName}'));
  }
  return CustomNodeEditor(
    nodeId: selectedNode.id,
    params: params,            // may be empty
    model: model,
  );
```

`null` ⇒ genuinely not a custom node (keep the old message). `[]` ⇒ a custom
node with no simple-typed parameters — `CustomNodeEditor` renders a short
italic note (*"This custom node has no editable parameters."*).

### Input widgets: the "no unset state" constraint

The 7 shared input widgets (`FloatInput`, `IntInput`, `StringInput`,
`IVec2Input`, `IVec3Input`, `Vec2Input`, `Vec3Input`) and the two matrix cells
(`IntMatrixCell`, `FloatMatrixCell`) all share one architecture:

- `value` is **required and non-nullable**; there is no "unset" representation.
- The `TextEditingController` is seeded from `widget.value.toString()` and is
  **never empty** — so Flutter's native `hintText` placeholder (which only
  shows on an empty controller) is unreachable, and none of them expose a
  `hintText` constructor param.
- Invalid input is *reverted to `widget.value`* — they rely on always having a
  fallback value.
- Compound widgets rebuild the whole value from the untouched components of
  `widget.value` (`IVec3Input` → `APIIVec3(x: newX, y: widget.value.y, z: widget.value.z)`).
- `bool` has no shared input widget — `bool_editor.dart` inlines a
  `CheckboxListTile`, and a checkbox has no empty state regardless.

A literal "start empty + ghost text" placeholder is therefore only cleanly
reachable for the three scalar text fields, and only by modifying those shared
widgets. It does **not** generalize to vectors (a half-typed/half-ghost vector
reads badly, and per-axis `onChanged` has no `widget.value` to read the other
axes from), to matrices (per-cell ghost text across a 3×3 grid is unreadable),
or to `bool`. So the panel uses a **uniform "pre-seed + dim"** placeholder
model instead — see below.

### New widget: `node_data/custom_node_editor.dart`

A `StatelessWidget` (rebuilt by the `Consumer` in `node_data_widget.dart` on
every model change). Builds a `Column` of one row per `APICustomNodeParam`.
Each row reuses the corresponding built-in editor's input widget **unmodified**
— `FloatInput`, `IntInput`, `StringInput`, `Vec2Input`/`Vec3Input` and their
integer siblings, the `IntMatrixCell`/`FloatMatrixCell` 3×3 grid, and a
`CheckboxListTile` for `bool` (matching `bool_editor.dart`).

For each row, `CustomNodeEditor` computes:

```
effectiveValue = stored_value ?? default_value ?? typeZero   // never null
isPlaceholder  = (stored_value == null)
```

and passes `effectiveValue` as the (always non-null) `value` to the shared
widget. No shared input widget is changed.

**Three visual states per row:**

| State | Condition | Rendering |
|---|---|---|
| **Stored** | `stored_value != null` | Full-opacity widget. Clear button (reset/✕ icon) visible. |
| **Placeholder** | `stored_value == null`, `is_wired == false` | Widget pre-filled with `effectiveValue`, wrapped in `Opacity(~0.55)`, **fully interactive** (no `IgnorePointer`). Row tagged "(default)". No clear button (nothing to clear). |
| **Wired** | `is_wired == true` | Widget pre-filled with `effectiveValue`, `Opacity(~0.45)` + **`IgnorePointer`**. Italic annotation *"Supplied by wired input. Disconnect to edit inline."* No clear button. The stored literal (if any) is **not** cleared — it must survive a disconnect. |

The placeholder and wired states both dim the widget, so the differentiator is
**interactivity + the row label** (`IgnorePointer` and the italic annotation on
wired rows; the "(default)" tag and a live cursor on placeholder rows), not
opacity alone.

**Why pre-seed gives "promote on first edit" for free:** because the widget is
already populated with `effectiveValue` (the resolved default), the user's
first edit produces a *complete, correct* payload — editing the X axis of a
pre-seeded `(3, 5, 8)` yields `(10, 5, 8)`, with the other axes carried through
by the widget's existing `onChanged` logic. `CustomNodeEditor`'s `onChanged`
handler simply calls `setCustomNodeLiteral` with that payload; the row
transitions Placeholder → Stored on the next refresh. No half-default /
half-typed compound value is ever possible.

**Clear button** — reset/✕ icon, shown only in the Stored state; calls
`clearCustomNodeLiteral`, returning the row to the Placeholder state (re-dimmed,
re-seeded from the resolved default).

**`typeZero` fallback** — when both `stored_value` and `default_value` are
`null` (default pin unconnected or its evaluation failed), `effectiveValue` is
the type's zero: `0`, `0.0`, `""`, `false`, `(0,0,0)`, identity matrix.

### Model methods (`structure_designer_model.dart`)

```dart
List<APICustomNodeParam>? getCustomNodeParams(BigInt nodeId) =>
    sd_api.getCustomNodeParams(nodeId: nodeId);

void setCustomNodeLiteral(BigInt nodeId, String paramName, APILiteralValue value) {
  sd_api.setCustomNodeLiteral(nodeId: nodeId, paramName: paramName, value: value);
  refreshFromKernel();
  notifyListeners();
}

void clearCustomNodeLiteral(BigInt nodeId, String paramName) {
  sd_api.clearCustomNodeLiteral(nodeId: nodeId, paramName: paramName);
  refreshFromKernel();
  notifyListeners();
}
```

(The setter/clear API functions already call `refresh_structure_designer_auto`
Rust-side; `refreshFromKernel()` pulls the refreshed scene back into the model.)

## Evaluation Priority (unchanged — for reference)

Already implemented in `parameter.rs::eval`:

1. **Wired pin connected** → evaluate the wire (highest priority).
2. **Literal stored** in `CustomNodeData.literal_values` → use it.
3. **`default` pin** of the parameter node inside the subnetwork (lowest).

The panel never has to enforce this — it only edits layer 2 and *reflects*
layer 1 (wired rows) and layer 3 (the placeholder seed value).

## Edge Cases

| Case | Behaviour |
|---|---|
| Node is not a custom node | `get_custom_node_params` → `None`; old "No editor available" message. |
| Custom node, zero simple params | `Some([])`; `CustomNodeEditor` shows the "no editable parameters" note. |
| Parameter retyped in subnetwork; stored `TextValue` no longer matches | Getter returns `stored_value: None` (placeholder shown); the stale `TextValue` stays in `literal_values` but `to_network_result` already ignores it at eval time. A subsequent edit overwrites it. |
| Parameter renamed in subnetwork | Orphan `literal_values` entry — inert (out of scope, see Scope). |
| `default` pin unconnected / errors | `default_value: None`; the row's Placeholder state seeds from `typeZero` instead. |
| Pin both wired and has a stored literal | Row renders in the Wired state (dimmed + `IgnorePointer`); the stored literal is preserved and re-activates on disconnect (wire wins at eval — unchanged). |
| `custom_node_type` cache empty | Getter falls back to the network's `NodeType`; if still unresolved, returns `Some([])`. |

## Files Touched

| File | Change |
|---|---|
| `rust/src/api/structure_designer/structure_designer_api_types.rs` | Add `APILiteralValue`, `APICustomNodeParam`, `APISimpleParamType`. |
| `rust/src/api/structure_designer/structure_designer_api.rs` | Add `get_custom_node_params`, `set_custom_node_literal`, `clear_custom_node_literal`; `TextValue ⇄ APILiteralValue` conversions. |
| `rust/src/structure_designer/structure_designer.rs` | Add `resolve_parameter_default` helper. |
| `lib/src/rust/**` | Regenerated by `flutter_rust_bridge_codegen generate`. |
| `lib/structure_designer/node_data/custom_node_editor.dart` | **New** — the auto-generated panel widget. |
| `lib/structure_designer/node_data/node_data_widget.dart` | `default:` arm dispatches to `CustomNodeEditor`. |
| `lib/structure_designer/structure_designer_model.dart` | `getCustomNodeParams` / `setCustomNodeLiteral` / `clearCustomNodeLiteral`. |

No changes to the evaluator, `parameter.rs`, `CustomNodeData`, the undo system,
serialization, or **any shared input widget** — the uniform "pre-seed + dim"
placeholder model reuses `FloatInput` / `IntInput` / `Vec3Input` / matrix cells
/ `CheckboxListTile` exactly as they are.

## Implementation Phases

Split into **two phases along the Rust/Flutter seam** — one PR each. The
feature is not large, so finer slicing (per-type or per-function) would be
artificial churn; two phases is the natural review/test/checkpoint boundary,
and the FRB codegen output is the clean handoff between them.

### Phase 1 — Rust core + FFI

The three API types, `get_custom_node_params` / `set_custom_node_literal` /
`clear_custom_node_literal`, the `resolve_parameter_default` helper, the
`TextValue ⇄ APILiteralValue` conversions, `flutter_rust_bridge_codegen
generate`, and the full Rust test suite (see Testing Strategy → Rust).

This half is fully verifiable on its own. It is also where the known design
risks live — `resolve_parameter_default`'s `top_level_parameters` shadowing
(the placeholder is *not* always exactly the `default` pin's value) and the
`&mut self` evaluation cost on every `get_custom_node_params` call — so shaking
those out at the FFI boundary, before any UI is built on top, is deliberate.

### Phase 2 — Flutter UI

`CustomNodeEditor`, the `structure_designer_model.dart` methods, and the
`node_data_widget.dart` `default:`-arm wiring, plus the Flutter integration
tests. The widget carries the real design subtlety (three visual states,
pre-seed + dim, placeholder-vs-wired differentiation) and gets a focused pass
against a known-good, already-tested API.

## Testing Strategy

### Rust (`rust/tests/structure_designer/`)

- `get_custom_node_params`:
  - Filters out complex-typed parameters; keeps all 10 simple types; preserves
    pin order.
  - `is_wired` reflects argument connection state.
  - `stored_value` reflects `literal_values`; a type-mismatched stored value is
    reported as `None`.
  - `default_value` resolves a literal `default` pin; `None` for an unconnected
    one.
  - Returns `None` for a built-in node.
- `set_custom_node_literal` / `clear_custom_node_literal`:
  - Round-trips through `literal_values`.
  - Pushes a `SetNodeData` undo command; `assert_undo_redo_roundtrip` restores
    the prior `literal_values` map.
  - A literal change re-evaluates the custom node and downstream.

### Flutter (`integration_test/`)

- Smoke: select a custom node with mixed simple/complex pins → panel shows one
  field per simple pin, none for complex pins.
- A param with no stored literal renders in the Placeholder state (dimmed,
  pre-seeded from the default, still editable); editing it transitions the row
  to the Stored state (full opacity, clear button appears).
- Edit a field → value persists across reselection; clear button returns the
  row to the Placeholder state; wiring a pin moves it to the Wired state
  (dimmed + non-interactive) without losing the stored literal.

## Migration

None. Existing `.cnnd` files already deserialize `CustomNodeData` with empty or
populated `literal_values`; the panel simply makes those values visible and
editable.
