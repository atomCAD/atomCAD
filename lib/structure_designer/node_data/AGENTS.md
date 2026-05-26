# Node Data Editors - Agent Instructions

Per-node-type property editor widgets. Each node type has a corresponding editor widget displayed in the properties panel when that node is selected.

## Files

- `node_data_widget.dart` - Router: selects correct editor widget by node type name
- `node_editor_header.dart` - Shared header (node name, type info)
- `node_description_button.dart` - Shows node description tooltip
- `network_description_editor.dart` - Editor for network-level description/summary
- `matrix_cell.dart` - Shared `IntMatrixCell` / `FloatMatrixCell` widgets (compact numeric input + scroll-to-step) used by the 3x3 matrix editors (`imat3_*_editor.dart`, `mat3_*_editor.dart`)
- `record_def_dropdown.dart` - Shared `RecordDefDropdown` widget: name-only dropdown of project record defs + "Edit definition…" affordance. Used by `record_construct_editor.dart`, `record_destructure_editor.dart`, `product_editor.dart` to bind their `schema` / `target` `String` properties.
- `closure_editor.dart` - Shared `ClosureShapeEditor` widget for **both** the `closure` and `apply` nodes (they store the same `{ kind, type_args, param_names }` data and differ only in inward/outward pin expansion). A `Kind` dropdown (the four HOF-shape templates plus `Custom`) over 1–2 `DataTypeInput` rows for the free type slots (preset branch), or a list of `_CustomParamRow`s (name + type + delete) plus a Return Type `DataTypeInput` (Custom branch). Preset branches show a read-only result line for fixed/derived results. `node_data_widget.dart` routes both `'closure'` and `'apply'` to it, wrapping `onChanged(kind, typeArgs, paramNames)` into `model.setClosureData` / `setApplyData` (read back via `getClosureData` / `getApplyData`). Manual smoke walkthrough: (a) drop a `closure`, switch to Custom, add two params, observe zone pins resize; (b) drop an `apply`, switch to Custom, attach the closure via `f`, run the network; (c) switch back to Map, observe param drop + undo restores it. See `doc/design_closures.md` and `doc/design_custom_closure_kind.md`.
- `*_editor.dart` - One file per node type (40+ editors)

## Adding a New Node Editor

1. Create `lib/structure_designer/node_data/my_node_editor.dart`
2. Register in `node_data_widget.dart` by adding a case in the node type switch

## Editor Pattern

```dart
class MyNodeEditor extends StatelessWidget {
  final StructureDesignerModel model;
  final NodeView node;

  // Build property controls that call model methods to update Rust state
  // After changes: model.refreshFromKernel()
}
```

Editors typically use shared widgets from `lib/common/` for numeric inputs, dropdowns, etc.

## Naming Convention

Editor files follow: `{node_type_name}_editor.dart`
- `sphere_editor.dart` → Sphere node
- `materialize_editor.dart` → Materialize node
- `vec3_editor.dart` → Vec3 node

## Node Types Without Custom Editors

Some nodes (like boolean operations: Union, Intersect, Diff) have no editable properties — they only receive wired inputs. These don't need dedicated editor files; `node_data_widget.dart` shows the default header only.

## "Disable on wired input" Pattern

For nodes whose stored property is overridden when an input pin is wired (`imat3_diag`, `mat3_diag`, `supercell`, `atom_replace`, …), the editor must render in a disabled state when that pin is connected — but **must not** clear the stored values, so they re-activate on disconnect. Use this shape:

1. **Detect connection** by walking `model.nodeNetworkView.wires` for `wire.destNodeId == nodeId && wire.destParamIndex == BigInt.from(<pin_index>)`. Cache the result locally in `build()` — don't store it on the widget.
2. **Wrap the editable region** in `Opacity(opacity: connected ? 0.5 : 1.0)` + `IgnorePointer(ignoring: connected, child: ...)`. This works for any inner widget, even ones that don't expose an `enabled` parameter (`SelectElementWidget`, etc.) — preferred over threading a new `enabled` parameter through shared widgets.
3. **Null out `onPressed`** on add/delete/edit buttons when connected (so they visibly disable rather than just ignore taps).
4. **Show an italic annotation** above the disabled region: `'<Property> supplied by \`<pin_name>\` input. Disconnect to edit inline.'`
5. **Never call the model's setter** to clear stored values when connecting. The Rust eval side handles "wired replaces stored"; the editor only owns the UI affordance.

The matching backend convention is documented in `rust/src/structure_designer/nodes/AGENTS.md` (matrix nodes' "wired input pin overrides the corresponding row/column/diagonal at eval"); the corresponding Rust subtitle drop is `get_subtitle()` returning `None` when `connected_input_pins.contains("<pin_name>")`.
