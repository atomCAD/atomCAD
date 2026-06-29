# Node Data Editors - Agent Instructions

Per-node-type property editor widgets. Each node type has a corresponding editor widget displayed in the properties panel when that node is selected.

## Files

- `node_data_widget.dart` - Router: selects correct editor widget by node type name
- `node_editor_header.dart` - Shared header (node name, type info)
- `node_description_button.dart` - Shows node description tooltip
- `network_description_editor.dart` - Editor for network-level description/summary
- `matrix_cell.dart` - Shared `IntMatrixCell` / `FloatMatrixCell` widgets (compact numeric input + scroll-to-step) used by the 3x3 matrix editors (`imat3_*_editor.dart`, `mat3_*_editor.dart`)
- `record_def_dropdown.dart` - Shared `RecordDefDropdown` widget: name-only dropdown of project record defs + "Edit definition…" affordance. Used by `record_construct_editor.dart`, `record_destructure_editor.dart`, `product_editor.dart` to bind their `schema` / `target` `String` properties.
- `closure_editor.dart` - `ClosureShapeEditor` widget for the **`closure`** node (`node_data_widget.dart`'s `'closure'` case). A `Kind` dropdown (the four HOF-shape templates plus `Custom`) over 1–2 `DataTypeInput` rows for the free type slots (preset branch), or a list of `_CustomParamRow`s (name + type + delete) plus a Return Type `DataTypeInput` (Custom branch). Preset branches show a read-only result line for fixed/derived results. Custom supports an empty parameter list (0-arity thunks render as `() → R` in the title). `onChanged(kind, typeArgs, paramNames)` is wired into `model.setClosureData` (read back via `getClosureData`). See `doc/design_closures.md`, `doc/design_custom_closure_kind.md`.
- `apply_editor.dart` - Placard-only widget for the **`apply`** node (`node_data_widget.dart`'s `'apply'` case). After `doc/design_function_pin_unification.md` Phase D, `apply` no longer has a user-set kind UI: its argument pin layout is **derived** by the Rust post-pass from whatever function value is wired into `f`. The placard renders "wire a function into `f`" guidance when `f` is disconnected, and a read-only summary of the wired source's signature when `f` is connected (the underlying `ApplyData` is kept for `.cnnd` back-compat but is not user-editable from this panel). Manual smoke walkthrough: (a) drop a 2-arg `closure` Custom `(x, y) → x*y`, wire to `apply.f`, observe two arg pins materialize; wire only `arg0` and confirm the apply output retypes to `Function((Int,) → Int)`. (b) Disconnect `f` and confirm the apply collapses back to a single-pin view. (c) Drop a 0-arity `closure` Custom (no params, return `Float`, body `42.0`), wire it into apply.f with no arg pins, confirm `apply` evaluates to `42.0`.
- `patch_build_editor.dart` / `patch_latticefill_editor.dart` - Editors for the surface-reconstruction patch nodes (`node_data_widget.dart`'s `'patch_build'` / `'patch_latticefill'` cases). `patch_build` is a single `FloatInput` (build threshold ε) plus a wiring hint; `patch_latticefill` has a passivate checkbox, a tolerance `FloatInput`, a **"Test height at lattice origin"** checkbox (`testHeightAtOrigin`, default off → target-derived cell-selection height; see `doc/design_patch_cell_selection.md`), a **compatibility badge** (`_CompatibilityBadge`), and a "Debug (cell selection)" group with two checkboxes (`debugProjectToTestPlane`, `debugShowFrontierTiles`). The badge reads `APIPatchLatticeFillData.report` (an `APICompatibilityReport?`): **red "No tiles placed" when `placedCells == 0`** (failure — nothing tiled, usually the wrong test-height mode for an off-origin target), green "Compatible" when tiles placed with no orphaned collars / over-coordination, amber "Check fit" otherwise, "not yet evaluated" when null. The report is **eval-populated**: the Rust node caches its last `CompatibilityReport` in a `#[serde(skip)] RefCell<Option<_>>` (same pattern as `MaterializeData::available_parameters`) and the getter reads it — so the badge refreshes whenever the node is displayed/evaluated. See `doc/design_surface_patches.md` §6 and `doc/design_patch_cell_selection.md`. **All booleans flow through `_commit(...)` → `model.setPatchLatticefillData`; adding a field to `APIPatchLatticeFillData` requires `flutter_rust_bridge_codegen generate`.**
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

**Counter-pattern — annotate-but-stay-enabled (`materialize.regions`).** Not every connected pin should disable the editor. `materialize_editor.dart` detects the optional `regions` pin (input index 6) with the same `nodeNetworkView.wires` walk, but when connected it **keeps the checkboxes enabled** and instead shows an italic *"Regions override these settings inside their volumes."* annotation above them. The reason is semantic: per-region records *layer on top of* the node's own settings (the "root"), so the root checkboxes stay meaningful — they are not shadowed by the wired data the way `atom_replace.rules` shadows the stored rule list. So: wired pin ⇒ disable only when the wire *replaces* the stored data; annotate-only when it *augments* it. See `doc/design_blueprint_region_atom_edits.md` §B7. (The `freeze`/`unfreeze` nodes from Part A of that doc have **no editor** — they are stateless and property-less, like the boolean-op nodes above.)

## Structural Function / Iter types in `DataTypeInput`

`lib/inputs/data_type_input.dart` exposes `Iter[T]` and `Function((args…) → R)` as first-class dropdown branches alongside the existing `Custom...` text escape hatch. For both variants the inline UI is a one-line **compact signature + ✎ Edit** row, and the full structural editor lives behind a `DraggableDialog` opened by `showTypeEditorDialog` (`lib/inputs/type_editor_dialog.dart`). The dialog hosts `FunctionTypeInput` for Function or a nested `DataTypeInput` for Iter; nested structural types open further dialogs (e.g. a Function whose return type is another Function = 2 stacked dialogs). Edits commit live (no Apply/Cancel — Ctrl+Z handles regret). The compact signature is rendered by `apiDataTypeToString` (top-level in `type_editor_dialog.dart`).

`children` defaults are seeded at the single dropdown-change boundary, so the dialog's editors can rely on the encoding from `doc/design_structural_function_and_iter_types.md` §"Children encoding" (Iter ⇒ 1 child; Function ⇒ N+1 children with the return type last). The outer Array checkbox preserves `children` across toggles so `Array[Iter[T]]` stays well-formed.

Manual smoke walkthrough (Phase 2 of the design doc):

1. Drop a `parameter` node, open its type picker, switch to `Iter[T]`. The inline summary row should read `Iter[Float]`. Click ✎ Edit → a draggable dialog opens with the element-type picker; change the element type to `Int`. The summary should update to `Iter[Int]` after closing the dialog (live commit also visible while the dialog is open).
2. On the same `parameter`, switch the type to `Function(args… → R)`. Inline summary: `(Float) → Float`. Open the dialog → add a parameter, change types to produce `(Int, Bool) → String`, close. Connect a matching wire to confirm the type took effect.
3. **Nested-dialog drill-in.** From the Function dialog above, change Parameter 1's type to `Iter[Int]` — clicking ✎ Edit on its inline summary should stack a *second* dialog. Close both; the outer summary should now read `(Iter[Int], Bool) → String`.
4. Drop a `closure` node and switch its kind to `Custom`. Each `_CustomParamRow`'s type slot is a `DataTypeInput` and inherits the dialog affordance for free — confirm that picking `Function((Int) → Float)` in a param row works and produces a healthy graph.
5. Open an old `.cnnd` that contains a `Custom: Iter[Int]` type. On next paint the picker should render the structural `Iter[T]` summary row with `Int` inside — no migration step required (the Rust→API converter promotes Iter/Function automatically).

See `doc/design_structural_function_and_iter_types.md` for the full design.
