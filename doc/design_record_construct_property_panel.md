# Design: Auto-Generated Field Editor for `record_construct`

## Problem Statement

`record_construct` has one input pin per field of the chosen record type def
(`schema`). Currently its property panel (`record_construct_editor.dart`) only
exposes the schema dropdown — the per-field input pins are wire-only. To set a
field to a constant, the user must wire a literal node (`float`, `vec3`, …)
into each pin, exactly as custom nodes required before
[[design_custom_node_property_panel]].

We want `record_construct` to get the **same auto-generated, per-pin inline
editing** that custom nodes now have: one input row per field whose data type is
a simple scalar/vector/matrix type, with the same three visual states
(Stored / Placeholder / Wired) and the same `wired > literal > fallback`
eval priority. Wire-only behaviour for complex fields stays.

## Relationship to the Custom Node Editor

This is the **second** instance of the same UI pattern. The first is documented
in `doc/design_custom_node_property_panel.md`, and shipped recently as
`CustomNodeEditor` + `APILiteralValue` + the FFI getter/setter/clear triad.

Both editors render the same shape:

- A list of typed input pins.
- For each pin: simple-typed pins get an inline editor; complex/abstract pins
  are skipped (they stay wire-only).
- Three visual states per row (Stored / Placeholder / Wired), driven by `(stored
  literal?, wire connected?)`.
- An eval rule: a wired pin always wins; otherwise the stored literal is used;
  otherwise a fallback.

The differences are narrow and local:

| Aspect | Custom node | `record_construct` |
|---|---|---|
| Pin source | Parameter nodes in a subnetwork | Fields of a `RecordTypeDef` |
| "Default" layer | The parameter node's `default` input pin, resolved by evaluating the subnetwork in isolation | **None.** No third layer — fallback is the simple-type zero. |
| Storage owner | `CustomNodeData.literal_values` (already exists) | `RecordConstructData.literal_values` (**new field**) |
| Pin index → name | `parameters[i].name` (cached on the custom node) | `def.fields[i].0` (authored field order, already drives the pin layout) |
| Eval site that consults the literal | `parameter.rs::eval` (already implemented) | `record_construct.rs::eval` (**new branch**) |
| Getter FFI cost | `&mut self` — needs to evaluate each parameter's `default` pin | **`&self`** — no evaluation, just walk the def's fields and check the wire map |
| Existing editor | None (fell through `default:` arm) | `RecordConstructEditor` exists, shows only the schema dropdown |

The pattern is the same; the cheap half is what's new.

## Code Reuse Strategy

A direct copy of `CustomNodeEditor` and a parallel set of FFI functions would
work, but would leave two visually-identical 360-line widgets and two
parallel-but-not-quite-identical FFI getter/setter triples to maintain. Instead,
factor out the schema-source-agnostic core and reuse it.

### Shared on the Flutter side

`CustomNodeEditor` becomes a thin wrapper over a new `LiteralFieldsEditor`
widget (in `lib/structure_designer/node_data/literal_fields_editor.dart`) that:

- Takes `List<APILiteralField>` (the renamed type — see below), a header
  widget, and three callbacks: `onSet(fieldName, value)`, `onClear(fieldName)`,
  and an "empty list" message.
- Owns all the visible behaviour: three-state rendering, `effectiveValue`
  computation, `Opacity` / `IgnorePointer` wrapping, the clear button, the
  `(default)` annotation, the typeZero fallback, the per-row `ValueKey`s that
  keep input widget state alive across Placeholder → Stored, the `_buildInput`
  switch, the `_buildIMat3` / `_buildMat3` grids, the defensive `_asBool` …
  `_asMat3` extractors, and the `_typeZero` table.

`CustomNodeEditor` becomes:

```dart
return LiteralFieldsEditor(
  header: NodeEditorHeader(title: 'Custom Node Properties', nodeTypeName: ...),
  fields: params,                      // already APILiteralField after rename
  onSet:   (name, v) => model.setCustomNodeLiteral(nodeId, name, v),
  onClear: (name)    => model.clearCustomNodeLiteral(nodeId, name),
  emptyMessage: 'This custom node has no editable parameters.',
);
```

`RecordConstructEditor` becomes (after picking a schema):

```dart
return Column(children: [
  // Existing schema dropdown — unchanged.
  RecordDefDropdown(...),
  const SizedBox(height: 8),
  LiteralFieldsEditor(
    header: const SizedBox.shrink(),     // schema dropdown is the header
    fields: fields,                      // from getRecordConstructFields
    onSet:   (name, v) => model.setRecordConstructLiteral(nodeId, name, v),
    onClear: (name)    => model.clearRecordConstructLiteral(nodeId, name),
    emptyMessage: 'This record type has no editable fields.',
  ),
]);
```

No behaviour changes for the custom-node case — the lift is a pure refactor.

### Shared on the Rust side

- **`APILiteralValue` and `APISimpleParamType` are reused verbatim.** Same
  simple-type coverage (`bool`, `int`, `float`, `str`, `ivec2`/`ivec3`,
  `vec2`/`vec3`, `imat3`/`mat3`); same FRB enum shape; same `TextValue ⇄
  APILiteralValue` conversion functions.

- **`APICustomNodeParam` is renamed to `APILiteralField`**, and its
  `default_value` field is **made `Option`-but-always-`None` for the
  `record_construct` case** (already `Option<APILiteralValue>` today, so no
  schema change). For custom nodes, the field continues to carry the resolved
  default-pin value; for `record_construct`, it is always `None` (there is no
  default layer to resolve). The `LiteralFieldsEditor` reads
  `stored_value ?? default_value ?? typeZero`, and `default_value == null` for
  record_construct collapses cleanly to typeZero.

- **The setter/clear API functions share their core via a small helper.** Both
  setter/clear pairs are mechanically identical except for the downcast type
  and the per-node-data `literal_values` accessor. Factor out:

  ```rust
  /// Mutate a node's literal_values map in place under undo. T is the
  /// node data struct (CustomNodeData or RecordConstructData) and `f` is
  /// the in-place mutation closure.
  fn with_node_literal_values<T, F>(node_id: u64, f: F)
  where
      T: NodeData + Clone + Default + 'static,
      F: FnOnce(&mut HashMap<String, TextValue>);
  ```

  Both `set_*_literal` / `clear_*_literal` functions reduce to a one-liner
  closure. The undo path stays uniform — `set_node_network_data` already
  handles the snapshot-and-push.

- **The getter FFI functions stay separate** —
  `get_custom_node_params(node_id) -> Option<Vec<APILiteralField>>` does
  subnetwork evaluation it doesn't need to do for records;
  `get_record_construct_fields(node_id) -> Option<Vec<APILiteralField>>` is a
  cheap `&self`-only walk of `def.fields` against
  `RecordConstructData.literal_values` and `node.arguments`. Sharing them would
  push the cost of `resolve_parameter_default` onto record_construct for no
  benefit. Both return the same `Vec<APILiteralField>`, so the Flutter side is
  identical.

### What is **not** shared

- `RecordConstructEditor` keeps the schema dropdown above the
  `LiteralFieldsEditor` — the dropdown is a `record_construct`-specific
  affordance with its own "Edit definition…" path, not a generic feature of the
  pattern.
- The Rust eval site (`record_construct.rs::eval` vs `parameter.rs::eval`) is
  the same idea but on different node types; there's no shared helper to
  factor, because each consults its own `node.data` downcast.

## Scope

### In scope

- Add `literal_values: HashMap<String, TextValue>` to `RecordConstructData`.
- Add the `wired > literal > pass-None-through` check inside
  `record_construct.rs::eval` (today it just calls `evaluate_arg` and bails on
  `None`).
- Refactor `CustomNodeEditor` to delegate to a new `LiteralFieldsEditor`
  widget.
- Wire `LiteralFieldsEditor` into `RecordConstructEditor` for the
  `schema`-chosen state.
- Add a new FFI triple — `get_record_construct_fields`,
  `set_record_construct_literal`, `clear_record_construct_literal` — modelled
  on the custom-node triple but with the `&self` getter and a shared
  setter/clear helper (see "Code Reuse Strategy" above).

### Editable ("simple") field types

Identical to [[design_custom_node_property_panel]]: `bool`, `int`, `float`,
`string`, `ivec2`, `ivec3`, `vec2`, `vec3`, `imat3`, `mat3`. Every other field
type — `Blueprint`, `Crystal`, `Molecule`, `Structure`, `Array[..]`, `Iter[..]`,
nested `Record(..)`, `Function(..)`, abstract supertypes, … — is omitted from
the panel and stays wire-only.

### Out of scope

- Renaming a field inside the record def orphans its `literal_values` entry
  (keyed by name). Orphan entries are inert — `record_construct.rs::eval`
  ignores any key that isn't a current field, and the getter never surfaces
  them — but they linger in the `.cnnd` file. Acceptable for v1; a future
  cleanup pass in `repair_node_network`'s record-node refresh could prune them.
  Note that `repair_node_network` already re-derives `record_construct` pin
  layouts when the def changes (see `rust/src/structure_designer/AGENTS.md` —
  Record Type Defs), so this is the natural place to prune.
- Inline editing of fields whose type is one of the complex/abstract types
  listed above. (Nested records in particular are wire-only — you can already
  wire a `record_construct` for the nested type.)
- Changing the schema dropdown's behaviour. Switching the schema may make
  existing literal entries irrelevant (the field names change); they orphan
  per the rule above.

## Architecture

```
┌─ Flutter ───────────────────────────────────────────────────┐
│ node_data_widget.dart                                       │
│   ├─ "record_construct" arm                                 │
│   │    └─ RecordConstructEditor                             │
│   │         ├─ RecordDefDropdown   (unchanged)              │
│   │         └─ LiteralFieldsEditor (NEW shared widget)      │
│   │              fields: getRecordConstructFields(nodeId)   │
│   │              onSet:   setRecordConstructLiteral(...)    │
│   │              onClear: clearRecordConstructLiteral(...)  │
│   └─ default: arm  (custom nodes)                           │
│        └─ CustomNodeEditor                                  │
│             └─ LiteralFieldsEditor (SAME widget)            │
│                  fields: getCustomNodeParams(nodeId)        │
│                  onSet:   setCustomNodeLiteral(...)         │
│                  onClear: clearCustomNodeLiteral(...)       │
├─ FFI (structure_designer_api.rs) ───────────────────────────┤
│   get_record_construct_fields(node_id)        — &self       │
│   set_record_construct_literal(node_id, ...)                │
│   clear_record_construct_literal(node_id, ...)              │
│   (existing custom-node triple unchanged)                   │
├─ Rust core ─────────────────────────────────────────────────┤
│   RecordConstructData.literal_values  (NEW field)           │
│   record_construct.rs::eval — wired > literal > None        │
│   shared helper for setter/clear undo path                  │
└─────────────────────────────────────────────────────────────┘
```

## FFI Surface

### Renamed / reused API types (`structure_designer_api_types.rs`)

```rust
/// Renamed from APICustomNodeParam. Same shape; now used by both panels.
///
/// `default_value` has a uniform semantic across both call sites: it is
/// `Some(..)` iff a resolvable default layer exists behind the pin. For
/// custom nodes this is the value produced by the parameter node's
/// `default` input pin; for `record_construct` it is always `None` (no
/// default layer). `LiteralFieldsEditor` reads this as both the
/// Placeholder pre-fill (when `Some`) and the row's label cue:
///
/// - `Some(..)` → Placeholder pre-fill is the default; row labeled
///   `(default)`; eval falls back to this value when unwired/unstored.
/// - `None`     → Placeholder pre-fill is `typeZero`; row labeled
///   `(unset)`; eval short-circuits at this field with `None`.
pub struct APILiteralField {
    pub name: String,
    pub data_type: APISimpleParamType,
    pub stored_value: Option<APILiteralValue>,
    pub default_value: Option<APILiteralValue>,
    pub is_wired: bool,
}

// APILiteralValue and APISimpleParamType — unchanged from
// design_custom_node_property_panel.md.
```

Renaming `APICustomNodeParam → APILiteralField` is a one-shot churn on the
custom-node call sites (the getter return type, the Flutter import, the
`CustomNodeEditor` parameter). Worth doing because the new name is the
no-longer-customised concept; keeping the old name would mislead the reader of
`RecordConstructEditor`.

### New API functions (`structure_designer_api.rs`)

```rust
/// Returns `None` if `node_id` is not a `record_construct` node, or if its
/// chosen `schema` is empty / not in the registry. Returns `Some(vec)` —
/// possibly empty — listing only the def's simple-typed fields, in authored
/// field order.
///
/// Pure read — `&self`. No subnetwork evaluation.
#[flutter_rust_bridge::frb(sync)]
pub fn get_record_construct_fields(node_id: u64) -> Option<Vec<APILiteralField>>;

/// Inserts/updates `RecordConstructData.literal_values[field_name]`.
/// Routes through `set_node_network_data` for free SetNodeData undo +
/// `refresh_structure_designer_auto`.
#[flutter_rust_bridge::frb(sync)]
pub fn set_record_construct_literal(node_id: u64, field_name: String, value: APILiteralValue);

/// Removes `RecordConstructData.literal_values[field_name]`. Same path.
#[flutter_rust_bridge::frb(sync)]
pub fn clear_record_construct_literal(node_id: u64, field_name: String);
```

### `get_record_construct_fields` algorithm

Runs inside `with_cad_instance` — read-only.

1. Resolve the active network's node `node_id`. If its `node_type_name` is not
   `"record_construct"` → return `None`.
2. Downcast `node.data` to `RecordConstructData`. (Should always succeed for
   `record_construct`; return `None` defensively if not.)
3. Look up the schema via `registry.lookup_record_type_def(&data.schema)`
   (the unified accessor — see `node_type_registry`'s built-in record-def
   support). If `None` → return `None`.
4. For each `(field_name, field_type)` in `def.fields` at index `i`:
   - Map `field_type` → `APISimpleParamType`. If it is not one of the simple
     types, **skip** this field.
   - `is_wired = !node.arguments[i].is_empty()`. The `i` here is the same
     index `record_construct.rs::eval` uses when calling `evaluate_arg(...,
     param_index)`, so the wire-state check stays consistent with eval.
   - `stored_value`: `data.literal_values.get(field_name)`, converted to
     `APILiteralValue`. If the stored `TextValue` does not match the current
     `field_type` (field was retyped in the def), treat as `None`.
   - `default_value`: always `None` — there is no default layer.
5. Return `Some(vec)`.

This is a strict read; everything routes through `with_cad_instance`, not
`with_mut_cad_instance`. Cheap to call on every panel rebuild.

### Setter / clear — shared core

Both follow the established primitive-setter pattern, factored once:

```rust
fn with_record_construct_literal_values<F>(node_id: u64, f: F)
where F: FnOnce(&mut HashMap<String, TextValue>)
{
    with_mut_cad_instance(|cad| {
        let Some(mut data) = clone_node_data::<RecordConstructData>(cad, node_id) else { return; };
        f(&mut data.literal_values);
        cad.structure_designer.set_node_network_data(node_id, Box::new(data));
        refresh_structure_designer_auto(cad);
    });
}

pub fn set_record_construct_literal(node_id: u64, field_name: String, value: APILiteralValue) {
    with_record_construct_literal_values(node_id, |map| {
        map.insert(field_name, value.into());
    });
}

pub fn clear_record_construct_literal(node_id: u64, field_name: String) {
    with_record_construct_literal_values(node_id, |map| {
        map.remove(&field_name);
    });
}
```

If the custom-node setter/clear pair is refactored onto the same `clone +
mutate + set_node_network_data` shape (parameterised by data type), the helper
can be made generic in the data type and shared across both pairs. Worth doing
only if a third user appears; for two it is borderline. **Defer until we see a
third call site** — keep the helpers per node type for now.

`set_node_network_data` already snapshots and pushes a `SetNodeData` undo
command. No new undo command type is needed; a record-construct literal edit
is undone/redone exactly like a custom-node literal edit or a `float` value
edit.

## Rust Core Changes

### `RecordConstructData` (`nodes/record_construct.rs`)

Add the literal map alongside the existing `schema` field, with `#[serde(default)]`
for backward compat with old `.cnnd` files:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecordConstructData {
    #[serde(default)]
    pub schema: String,

    /// Per-field stored literal values. Consulted in eval when the
    /// corresponding input pin is unwired. Keyed by field name.
    /// Entries whose key isn't a current field are inert (orphan-tolerant).
    #[serde(default)]
    pub literal_values: HashMap<String, TextValue>,
}
```

`Default` derive still works (`HashMap::default()` is empty).

### `record_construct.rs::eval` — wired > literal > None

Today the eval loop is:

```rust
for (param_index, (field_name, _)) in def.fields.iter().enumerate() {
    let value = network_evaluator.evaluate_arg(network_stack, node_id, registry, context, param_index);
    match &value {
        NetworkResult::None  => return EvalOutput::single(NetworkResult::None),
        NetworkResult::Error(_) => return EvalOutput::single(value),
        _ => {}
    }
    fields.push((field_name.clone(), value));
}
```

The new behaviour interposes the literal check between the wire and the bail.
Both branches below preserve the existing "any `None`/`Error` short-circuits
the whole record" semantics.

```rust
for (param_index, (field_name, field_type)) in def.fields.iter().enumerate() {
    let value = if !node.arguments[param_index].is_empty() {
        // Wired pin — evaluate normally.
        network_evaluator.evaluate_arg(network_stack, node_id, registry, context, param_index)
    } else if let Some(text_value) = self.literal_values.get(field_name) {
        // Unwired but a stored literal exists — try to coerce it to the
        // field's type. If coercion fails (e.g. field retyped, stale entry),
        // fall through to evaluate_arg, which will return None.
        text_value.to_network_result(field_type)
            .unwrap_or_else(|| network_evaluator.evaluate_arg(
                network_stack, node_id, registry, context, param_index))
    } else {
        // Unwired, no stored literal — same as today.
        network_evaluator.evaluate_arg(network_stack, node_id, registry, context, param_index)
    };
    match &value {
        NetworkResult::None     => return EvalOutput::single(NetworkResult::None),
        NetworkResult::Error(_) => return EvalOutput::single(value),
        _ => {}
    }
    fields.push((field_name.clone(), value));
}
```

`node` is reachable via the standard pattern used elsewhere in the file
(`network_stack.last().unwrap().node_network.nodes.get(&node_id)`).

Sanity check this against the wired case: when the user wires a `vec3` literal
*and* has a stored vec3 literal for the same field, the wire still wins —
exactly as the panel's Wired-row rendering claims. The stored literal is not
cleared; disconnecting the wire re-activates it.

## Flutter UI

### `LiteralFieldsEditor` (new shared widget)

`lib/structure_designer/node_data/literal_fields_editor.dart`.

Lifted from `CustomNodeEditor` essentially as-is. Public API:

```dart
class LiteralFieldsEditor extends StatelessWidget {
  final Widget header;                                // SizedBox.shrink() to omit
  final List<APILiteralField> fields;                 // may be empty
  final String emptyMessage;
  final void Function(String name, APILiteralValue value) onSet;
  final void Function(String name) onClear;
}
```

Body identical to the current `CustomNodeEditor.build` minus the
`NodeEditorHeader` hardcoding and minus the `nodeId` / `model` field — those
move into the parent's callbacks. All private helpers (`_buildRow`,
`_buildInput`, `_buildIMat3`, `_buildMat3`, the `_as*` extractors, `_typeZero`)
move into this file. **No behavioural change** for the custom-node case.

### `CustomNodeEditor` (refactor)

`lib/structure_designer/node_data/custom_node_editor.dart` becomes a thin
adapter:

```dart
class CustomNodeEditor extends StatelessWidget {
  final BigInt nodeId;
  final String nodeTypeName;
  final List<APILiteralField> params;
  final StructureDesignerModel model;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: LiteralFieldsEditor(
        header: NodeEditorHeader(
          title: 'Custom Node Properties',
          nodeTypeName: nodeTypeName,
        ),
        fields: params,
        emptyMessage: 'This custom node has no editable parameters.',
        onSet:   (name, v) => model.setCustomNodeLiteral(nodeId, name, v),
        onClear: (name)    => model.clearCustomNodeLiteral(nodeId, name),
      ),
    );
  }
}
```

The `node_data_widget.dart` `default:` arm is unchanged.

### `RecordConstructEditor` (extend)

```dart
class RecordConstructEditor extends StatelessWidget {
  // existing fields: nodeId, data, model
  @override
  Widget build(BuildContext context) {
    if (data == null) return const Center(child: CircularProgressIndicator());
    final schemaChosen = data!.schema.isNotEmpty;
    final fields = schemaChosen
        ? model.getRecordConstructFields(data!.nodeId)  // pass nodeId via constructor
        : null;
    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(title: 'Record Construct', nodeTypeName: 'record_construct'),
          const SizedBox(height: 8),
          RecordDefDropdown(
            value: data!.schema,
            label: 'Schema',
            emptyHint: '— No schema chosen —',
            model: model,
            onChanged: (newName) => model.setRecordConstructData(
              nodeId, APIRecordSchemaData(schema: newName)),
          ),
          if (fields != null) ...[
            const SizedBox(height: 12),
            LiteralFieldsEditor(
              header: const SizedBox.shrink(),
              fields: fields,
              emptyMessage: 'This record type has no editable fields.',
              onSet:   (name, v) => model.setRecordConstructLiteral(nodeId, name, v),
              onClear: (name)    => model.clearRecordConstructLiteral(nodeId, name),
            ),
          ],
        ],
      ),
    );
  }
}
```

`getRecordConstructFields` returning `null` ⇒ the schema dropdown is empty or
points at a missing def; render only the dropdown (existing behaviour). `[]` ⇒
chosen schema has no simple-typed fields; show the empty-message note.

### Three visual states (recap)

Same rendering rules as [[design_custom_node_property_panel]], with the
Placeholder annotation derived from `default_value` rather than hardcoded:

| State | Condition | Rendering |
|---|---|---|
| **Stored** | `stored_value != null` | Full-opacity widget. Clear (✕) button visible. |
| **Placeholder** | `stored_value == null`, `is_wired == false` | Pre-filled with `default_value ?? typeZero`, `Opacity(~0.55)`, fully interactive. Row labeled `(default)` when `default_value != null`, otherwise `(unset)`. No clear button. |
| **Wired** | `is_wired == true` | Pre-filled, `Opacity(~0.45)` + `IgnorePointer`. Italic "Supplied by wired input. Disconnect to edit inline." No clear button. The stored literal (if any) is **not** cleared. |

For `record_construct`, `default_value` is always `None`, so every
Placeholder row reads `(unset)` and is pre-seeded with `typeZero`. The
typeZero pre-fill is **only a typing seed** — unlike the custom-node case,
eval does not use it; an unwired-and-unstored simple field produces
`NetworkResult::None` and short-circuits the whole record (see [Evaluation
Priority](#evaluation-priority)). Editing the row promotes it to Stored,
which is the user's signal that this field should contribute a value.

Pre-seed + dim still gives the "promote on first edit produces a complete
payload" property — same as for custom nodes — because the input widget is
seeded with `typeZero` before the user touches it.

### Model methods (`structure_designer_model.dart`)

```dart
List<APILiteralField>? getRecordConstructFields(BigInt nodeId) =>
    sd_api.getRecordConstructFields(nodeId: nodeId);

void setRecordConstructLiteral(BigInt nodeId, String fieldName, APILiteralValue value) {
  sd_api.setRecordConstructLiteral(nodeId: nodeId, fieldName: fieldName, value: value);
  refreshFromKernel();
  notifyListeners();
}

void clearRecordConstructLiteral(BigInt nodeId, String fieldName) {
  sd_api.clearRecordConstructLiteral(nodeId: nodeId, fieldName: fieldName);
  refreshFromKernel();
  notifyListeners();
}
```

The existing custom-node model methods change only by `APICustomNodeParam →
APILiteralField` in their return types (FRB-regenerated typedef on the Dart
side).

## Evaluation Priority

```
wired pin connected  >  stored literal in literal_values  >  NetworkResult::None
```

`record_construct` has **no** "default pin" layer — that concept comes from
parameter nodes inside a subnetwork, which don't exist here. The
`NetworkResult::None` fallback is unchanged from today and still
short-circuits the whole record (any unwired-and-unliteralled simple field
yields a None record output).

## Edge Cases

| Case | Behaviour |
|---|---|
| Node is not `record_construct` | `get_record_construct_fields` → `None`; the existing `RecordConstructEditor` only runs for `record_construct` nodes, so this is mainly a defensive check. |
| Schema empty or dangling | Getter returns `None`; only the schema dropdown is rendered (existing UX). |
| Schema chosen, zero simple fields | `Some([])`; `LiteralFieldsEditor` shows the empty-fields note. |
| Field retyped in the def; stored `TextValue` no longer matches | Getter returns `stored_value: None` (Placeholder shown); the stale `TextValue` stays in `literal_values` but `to_network_result` ignores it at eval time. A subsequent edit overwrites it. |
| Field renamed in the def | Orphan `literal_values` entry — inert (out of scope, see Scope). |
| Pin both wired and has a stored literal | Row renders Wired (dimmed + `IgnorePointer`); the stored literal is preserved and re-activates on disconnect (wire wins at eval). |
| Schema switched to a different def | All current `literal_values` entries are likely to orphan against the new field set — same handling as a rename, inert and surfaceable on next edit. A future cleanup pass could prune on schema change. |
| Complex-typed field (e.g. nested `Record`, `Array`, abstract phase type) | Filtered out of the getter result; the pin stays wire-only with no inline row. |

## Files Touched

| File | Change |
|---|---|
| `rust/src/structure_designer/nodes/record_construct.rs` | Add `literal_values` field; thread `wired > literal > None` into `eval`. |
| `rust/src/api/structure_designer/structure_designer_api_types.rs` | Rename `APICustomNodeParam → APILiteralField`. `APILiteralValue` / `APISimpleParamType` unchanged. |
| `rust/src/api/structure_designer/structure_designer_api.rs` | Add `get_record_construct_fields` / `set_record_construct_literal` / `clear_record_construct_literal`; rename type in custom-node getter return. |
| `lib/src/rust/**` | Regenerated by `flutter_rust_bridge_codegen generate`. |
| `lib/structure_designer/node_data/literal_fields_editor.dart` | **New** — shared widget, lifted from `custom_node_editor.dart`. |
| `lib/structure_designer/node_data/custom_node_editor.dart` | Reduce to thin adapter over `LiteralFieldsEditor`. No behavioural change. |
| `lib/structure_designer/node_data/record_construct_editor.dart` | Append `LiteralFieldsEditor` below the schema dropdown when a schema is chosen. |
| `lib/structure_designer/structure_designer_model.dart` | Add `getRecordConstructFields` / `setRecordConstructLiteral` / `clearRecordConstructLiteral`. |

No changes to the evaluator, the undo system, serialization layout for any
node other than `RecordConstructData` (where the new `#[serde(default)]` map
is backward-compatible with old files), or any of the shared input widgets.

## Implementation Phases

Two phases along the Rust/Flutter seam — same shape as
[[design_custom_node_property_panel]], plus a phase 0 refactor that ships
independently.

### Phase 0 — Rename + lift the shared widget

Pure refactor, zero behaviour change.

- Rename `APICustomNodeParam → APILiteralField` (Rust + regenerate).
- Create `LiteralFieldsEditor`; reduce `CustomNodeEditor` to a thin adapter.
- Re-run the existing custom-node panel integration tests against the
  refactored widget; nothing should change.

Shipping this on its own keeps the diff for phases 1–2 narrow and makes any
regression on the custom-node panel attributable to this phase alone.

### Phase 1 — Rust core + FFI

- `RecordConstructData.literal_values` field.
- Eval branch in `record_construct.rs::eval`.
- The three new FFI functions.
- `TextValue ⇄ APILiteralValue` conversions already exist from
  [[design_custom_node_property_panel]] — reused as-is.
- Rust test suite — see Testing.

Fully verifiable on its own at the FFI boundary.

### Phase 2 — Flutter UI

- Model methods on `StructureDesignerModel`.
- Wire `LiteralFieldsEditor` into `RecordConstructEditor`.
- Flutter integration tests.

## Testing Strategy

### Rust (`rust/tests/structure_designer/`)

- `get_record_construct_fields`:
  - Filters out complex/abstract field types; keeps all 10 simple types;
    preserves authored field order.
  - `is_wired` reflects argument connection state.
  - `stored_value` reflects `literal_values`; a type-mismatched stored value
    is reported as `None`.
  - `default_value` is always `None`.
  - Returns `None` for a non-`record_construct` node, an empty schema, or a
    dangling schema.
- `set_record_construct_literal` / `clear_record_construct_literal`:
  - Round-trip through `literal_values`.
  - Push a `SetNodeData` undo command;
    `assert_undo_redo_roundtrip` restores the prior `literal_values` map.
- `record_construct.rs::eval`:
  - Wired pin overrides stored literal.
  - Stored literal fills in for unwired simple field; resulting record carries
    the literal value.
  - Mismatched stored `TextValue` (field retyped) falls back to None and
    short-circuits the record — verifies the eval branch's defensive coercion.
  - Orphan `literal_values` entry (no matching field) is ignored.

### Flutter (`integration_test/`)

- Smoke: select a `record_construct` with a schema chosen → schema dropdown
  plus one field row per simple field; complex-typed fields are not rendered.
- Schema empty → only the dropdown is shown; choosing a schema reveals the
  field list.
- A field with no stored literal renders Placeholder (dimmed, pre-seeded with
  typeZero, still editable); editing transitions to Stored (full opacity,
  clear button appears).
- Edit a field → value persists across reselection and across switching to
  another node and back; clear button returns the row to Placeholder; wiring
  a pin moves it to Wired (dimmed + non-interactive) without losing the
  stored literal.
- A literal change re-evaluates downstream nodes (smoke-test that a
  `record_destructure` consumer reflects the new value).

The Phase 0 refactor reuses the existing custom-node panel integration tests
unchanged; they double as the regression net for `LiteralFieldsEditor`.

## Migration

None. Existing `.cnnd` files deserialize `RecordConstructData` with
`literal_values` defaulted to an empty map (via `#[serde(default)]`); the
panel simply makes the new map visible and editable. No version bump.
