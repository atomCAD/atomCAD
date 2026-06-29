# Structure Designer UI - Agent Instructions

Main application UI for the atomCAD structure designer. Provides the node network editor, 3D viewport, property panels, and network management.

## Subdirectory Instructions

- Working in `node_network/` → Read `node_network/AGENTS.md`
- Working in `node_data/` → Read `node_data/AGENTS.md`

## Directory Structure

```
structure_designer/
├── structure_designer.dart           # Main widget: menu bar + 3-panel layout
├── structure_designer_model.dart     # StructureDesignerModel: central state
├── structure_designer_viewport.dart  # 3D viewport with ray-cast interaction
├── main_content_area.dart            # Resizable split: viewport + (network editor | schema editor)
├── schema_editor.dart                # Record-def field editor (active when activeRecordDefName != null)
├── camera_control_widget.dart        # Camera view selector (ortho/perspective)
├── node_display_widget.dart          # Display policy buttons (Manual/Selected/Frontier)
├── atomic_structure_visualization_widget.dart  # Atom/bond 3D display
├── geometry_visualization_widget.dart          # Geometry 3D display
├── preferences_window.dart           # Settings dialog
├── factor_into_subnetwork_dialog.dart # Extract selection to subnetwork
├── extract_closure_to_network_dialog.dart # Name dialog for Closure→Network conversion
├── import_cnnd_library_dialog.dart   # Import from .cnnd library
├── identifier_validation.dart        # Field/identifier validation rules
├── namespace_utils.dart              # User-type-name validation (networks + record defs share one namespace)
├── node_network/                     # Node graph editor
├── node_data/                        # Per-node-type property editors
└── node_networks_list/               # Unified user-types panel (networks + record defs)
```

## Key Files

| File | Purpose |
|------|---------|
| `structure_designer.dart` | Top-level widget, menu bar (File/View/Edit), layout |
| `structure_designer_model.dart` | `ChangeNotifier` state: wraps all Rust API calls |
| `structure_designer_viewport.dart` | `CadViewport` subclass for 3D ray-cast interaction + guided placement dispatch |
| `main_content_area.dart` | Resizable split between viewport and node editor |

## State Management Pattern

`StructureDesignerModel` (extends `ChangeNotifier`) is the single source of truth:

```
User interaction → Model method → Rust API call → refreshFromKernel() → notifyListeners()
```

Access via `Provider.of<StructureDesignerModel>(context)` or `Consumer<StructureDesignerModel>`.

All Rust state is fetched into `NodeNetworkView` (the model's snapshot of current network state).

## Property Panel Scope (zones)

A node id is **not** unique across the network: HOF zone bodies have per-body id counters, so a body node and a top-level node can share a numeric id (see `rust/AGENTS.md` → "Addressing Nodes Across Scopes"). Every Rust API that addresses a node takes a `scope_path`, so the Flutter side must always pass the **right** scope or it reads/writes the wrong node (this caused the original zones bug — clicking a body `expr` showed the outer one / spun on a null forever).

- **`StructureDesignerModel.propertyEditorScopeChain`** is the scope of the node currently shown in the property panel. `NodeDataWidget.build` sets it from the *resolved selection* (`_findSelectedNode` returns `(node, scopeChain)`). All property `get*Data` / `set*Data` model methods key off `propertyEditorScopeChain` / `propertyEditorScopePath` — **not** `activeScopeChain`. The two diverge: clicking a body interior changes `activeScopeChain` (used by keyboard ops: delete / copy / paste) without changing the selection, so an ancestor node can stay selected while a different body is active.
- A **new node property editor** that fetches data via a direct FRB `getXxxData(...)` call inside `node_data_widget.dart` must pass `scopePath: model.propertyEditorScopePath` (or `scopePath: scopePath`, the local already declared in `_buildNodeEditor`). New `model.setXxxData` / `getXxxData` wrappers must forward `propertyEditorScopeChain`, mirroring the existing ones.
- FRB's `Uint64List` is `package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart show Uint64List`, **not** `dart:typed_data` — the analyzer treats them as distinct types at API call sites. Prefer `_scopeChainToBytes(...)` / `propertyEditorScopePath` over constructing one directly.

## Layout

Three-panel layout:
- **Left sidebar:** Display policy, camera controls, network list (tabs: List/Tree)
- **Main area:** Resizable split between 3D viewport and node network editor
- Supports vertical (side-by-side) and horizontal (stacked) layout modes

## Guided Atom Placement (in viewport)

The Add Atom tool in `structure_designer_viewport.dart` has a state-aware click dispatcher: click empty space → free placement; click existing atom → guided placement (Rust computes guide dot positions, Flutter dispatches click/cancel/place). Pointer-move events update cursor-tracked previews for free-sphere and free-ring modes.

The atom edit panel exposes three dropdowns for guided placement: **Bond Length** (Crystal / UFF), **Hybridization** (Auto / sp3 / sp2 / sp1), and **Bond Mode** (Covalent / Dative). All reset to defaults when switching tools. The corresponding model properties are passed through to the Rust API. Saturation feedback uses SnackBar notifications with context-aware messages.

Design doc: `doc/atom_edit/guided_atom_placement.md`.

## AddBond Tool (drag-to-bond + bond order)

The AddBond tool uses drag-to-bond interaction: pointer down on atom → drag → release on target atom to create bond. Flutter routes pointer down/move/up events to the Rust `add_bond_pointer_down/move/up` API. During drag, `pointer_move` returns `AddBondMoveResult` with 3D positions; Flutter projects these to screen space and draws a rubber-band line via `CustomPainter` (2D overlay, no Rust evaluation per frame).

The `BondOrderSelector` widget (shared between AddBond tool panel and Default tool bond-info panel) provides two rows of segmented buttons: common orders (Single/Double/Triple) and specialized orders (Quad/Aromatic/Dative/Metallic), acting as a single radio group.

Keyboard shortcuts: **D** switches to Default tool, **Q** switches to AddAtom tool, hold **J** for spring-loaded AddBond tool activation (deferred release during active drag); **1-7** set bond order in AddBond tool or change selected bond(s) order in Default tool. **Delete/Backspace** deletes selected atoms/bonds. Type element symbols (C, N, Si, etc.) to select elements in Default/AddAtom tools.

## Per-Atom Hybridization Override (in atom_edit panel)

The Default tool shows a hybridization selector (SegmentedButton: Auto|sp3|sp2|sp1) when atoms are selected. It reflects the common override of selected atoms, or shows empty selection (no segment highlighted) when atoms disagree. Clicking a segment calls `atomEditSetHybridizationOverride` for all selected atoms. The Add Atom tool has the same selector for guided placement; it also writes a stored override on the anchor atom at placement time. The atom hover tooltip and single-atom measurement display show the hybridization as "sp2 (override)" or "auto". Design doc: `doc/atom_edit/design_hybridization_override.md`.

Design doc: `doc/atom_edit/design_bond_creation_and_order.md`.

## Modify Measurement (in atom_edit panel)

The measurement card (shown when 2–4 atoms selected) includes a "Modify" button that opens a draggable dialog for entering a precise distance, angle, or dihedral value. The dialog adapts per measurement type: value field with validation, "Default" button (bond length from Crystal/UFF table, or UFF theta0 for angles; hidden for dihedral), radio buttons to choose which atom/arm/side moves (pre-selected from `lastSelectedResultAtomId`), and a "Move connected fragment" checkbox.

Model methods: `atomEditModifyDistance`, `atomEditModifyAngle`, `atomEditModifyDihedral`, `atomEditGetDefaultBondLength`, `atomEditGetDefaultAngle`. Rust moves atoms along bond axes (distance), rotates around vertex (angle), or rotates around central bond axis (dihedral). Fragment mode uses BFS graph distance to determine co-moving atoms.

Design doc: `doc/atom_edit/design_modify_measurement.md`.

## Placement Guideline tool (in atom_edit panel + viewport)

A **guideline** is a transient line that constrains atom placement to hard-to-hit positions (issue #368). It is a dedicated **fourth tool** (a 4th button in the atom_edit toolbar, `Icons.timeline`, F5; gated to `atom_edit`, not motif). The guideline is transient — not serialized, not undoable — and vanishes on tool switch / node deselect; **Clear** (or Escape) drops the line back to `Define`.

- **Phase-driven panel** (`atom_edit_editor.dart`, `_buildGuidelinePanel`, rendered from `_buildToolSpecificUI`'s `guideline` case): reads `atom_edit_api.getGuidelineToolView()` (an `APIGuidelineToolView?`) live each build and switches on `APIGuidelinePhase`:
  - **Define** (`_buildGuidelineDefine`): an instruction + a Create button labeled from `view.definingCount` (1 → "Directional line" + a direction `Vec3Input`/Normalize, 2 → "Center line", 3 → "Equidistant line"), enabled when `view.canCreate`. `_createGuideline` calls `model.guidelineCreateFromDefining(dir)` and SnackBars a non-empty error.
  - **Place** (`_buildGuidelinePlace`): a two-way `FloatInput` (key `guideline_position`, `view.t`) → `model.guidelineSetPosition`, an element selector, **Place atom** → `model.guidelinePlaceAtom`, and **Clear**.
  - **Move** (`_buildGuidelineMove`): the same `t` field (now the picked atom's live projection) + **Clear**.
- **Model methods** (`structure_designer_model.dart`): `guidelineCreateFromDefining` (returns the error string), `guidelineSetPosition`, `guidelinePlaceAtom`, `guidelineClear`, `guidelineSetEnteredDirection`, and `notifyGuidelineToolSync` (plain notify for live drag rebuilds). The guideline FFI is global (operates on the active node), so these take no `scope_path`.
- **Viewport** (`structure_designer_viewport.dart`): pointer interaction runs through a dedicated `_AtomEditGuidelineDelegate` (selected by `primaryPointerDelegate` when the active tool is `guideline`) that forwards down/move/up to `guidelinePointer{Down,Move,Up}` and cancel to `guidelineResetInteraction`. A move that returns `true` calls `renderingNeeded()` + `model.notifyGuidelineToolSync()` so the `t` field tracks the atom live. **Escape** (after guided-placement precedence) clears the guideline when `getGuidelineToolView() != null` via `model.guidelineClear()`. **F5** switches to the Guideline tool.

Design doc: `doc/atom_edit/design_atom_guidelines.md`.

## Click-to-Activate (in viewport)

When multiple nodes are visible, clicking on a non-active node's rendered output in the 3D viewport activates that node (two-step interaction: first click activates, second click performs the normal action). The interception happens in `onPointerDown` before delegate dispatch, calling `viewport_pick()` (Rust API) which returns `ActivateNode`, `Disambiguation`, `ActiveNodeHit`, or `NoHit`. A performance guard skips the pick when only 0–1 nodes are displayed.

When overlapping outputs are detected (within 0.1 Å), a disambiguation overlay popup (`_DisambiguationOverlay`) appears near the click with two actions per candidate node: name click (activate + scroll) and solo eye icon (activate + scroll + hide other overlapping nodes). If the active node is among the overlapping hits, the click passes through as normal.

**Scroll-to-node callback pattern:** The viewport calls `model.scrollToNode(nodeId)` after activation. `StructureDesignerModel.onScrollToNode` is a callback registered by `NodeNetworkState` during `initState` (and cleared in `dispose`). This bridges the viewport→model→node-network-widget communication without requiring the viewport to hold a `GlobalKey` to the node network. SnackBar feedback (`"Activated: {nodeName}"`) confirms the activation.

Design doc: `doc/design_click_to_activate_node.md`.

## Record Types

The user-types panel and main content area handle two kinds of user-defined types: node networks and record type defs.

- **`StructureDesignerModel.activeRecordDefName: String?`** — when non-null, `MainContentArea` swaps the network editor out for `SchemaEditor` (the record-def field list editor). Activating a network clears it; activating a record def sets it.
- **API types:** `APIDataTypeBase::Record` + `APIRecordSchemaData` carry record-typed pin info to the UI; `APIRecordTypeDef` / `APIRecordTypeDefField` carry schema definitions. Model methods: `addRecordTypeDef`, `deleteRecordTypeDef`, `renameRecordTypeDef`, `updateRecordTypeDef`, plus `setActiveRecordDefName`.
- **Per-node editors** for `record_construct`, `record_destructure`, `product` use the shared `RecordDefDropdown` (`node_data/record_def_dropdown.dart`) — a name-only dropdown bound to the node's `schema` / `target` `String` property, with an "Edit definition…" affordance that activates the bound def and switches to the schema editor.
- **DataTypeInput** (`lib/inputs/data_type_input.dart`) gains a Record branch that lists named record defs only. Anonymous record types exist in the type system (via `expr` literals) but are never authored from the Flutter UI. (`DataTypeInput` also gained structural `Iter[T]` and `Function((args…) → R)` branches — see "Structural Function / Iter types" below.)
- **`Optional[T]` fields** (`doc/design_optional_type.md`): `DataTypeInput` exposes `Optional[T]` as a dropdown base entry (like `Iter`), but **only when `allowOptional: true`** — set only by the record `SchemaEditor`, since `Optional` is a record-field modifier and never a pin type. The inner type is edited via the shared `showTypeEditorDialog` (nested `DataTypeInput`, passed `optionalInner: true` to hide the ill-formed inners `Optional`/`Iter`/`Unit`/`None`); the outer Array checkbox is hidden for Optional. In the `record_construct` panel an `Optional[T]` field renders as a plain `T` literal row (the Rust getter peels the Optional via `record_field_pin_type()`); the existing `LiteralFieldsEditor` tri-state (Stored / `(unset)` / Wired) provides the force-on / force-off / inherit UX — "unset" (Clear, no `literal_values` entry, no wire) means `None`/inherit.
- The user-types panel rejects names that collide across networks, record defs, or built-ins (single namespace).

Design doc: `doc/design_record_types.md`.

## Multi-Output Pin UI

- **Eye icon** is per output pin, not in the title bar. Each output pin row has its own eye toggle.
- **Multi-output nodes** (e.g. atom_edit) show pin names ("result", "diff") next to each output pin. Single-output nodes do not show pin names.
- **`NodeView.output_pins: Vec<OutputPinView>`** and **`displayed_pins: Vec<i32>`** from the Rust API.
- **`toggleOutputPinDisplay(nodeId, pinIndex)`** model method toggles individual pin visibility.
- **Wire rendering:** output pin y-offset is per-pin (same formula as input pins). `getNodeSize()` / `estimate_node_height()` use `max(inputs, outputs, minHeight)`.
- **`OutputPinView { name, data_type, index }`** API type for each output pin.

## Closures (function values)

The `closure` and `apply` nodes (plus the four HOFs' optional `f` input pin) expose first-class function values to the UI. See `doc/design_closures.md`, `doc/design_currying.md`, and `doc/design_function_pin_unification.md`.

- **API types:** `APIClosureKind` (`map` / `filter` / `fold` / `foreach` / **`custom`** — the four HOF shape templates plus a fully-flexible `Custom` kind with arbitrary param names/types, including 0-arity thunks), `APIClosureData` and `APIApplyData` (both `{ kind, type_args, param_names }`). `Function` data types surface through the existing `APIDataType` machinery and render amber. **`AnyFunction`** (input-only pin type used by `apply.f` and `map.f`) renders with the same amber color as `Function`; its tooltip is built from a node-specific extra line (apply: "apply will call it on the wired arguments"; map: "applied per element of the stream"). `APIDerivedShapeView` on `NodeView` carries `derived_from_input_pin: Option<String>` — `Some("f")` when the node's layout/output type is currently derived from a wired `f`, `None` otherwise — and drives the apply placard / map output-type display switch.
- **Model / API:** `setClosureData` / `setApplyData` are model methods (they forward `activeScopeChain` as `scope_path` to the Rust API); `getClosureData` / `getApplyData` are direct generated-API calls. `node_data_widget.dart` routes `'closure'` to the shared `ClosureShapeEditor` (Map/Filter/Fold/Foreach/Custom) and `'apply'` to `apply_editor.dart` (a placard — apply has no user-set kind UI; pins are derived from the wired `f` by the Rust post-pass).
- **Editor:** the closure shape editor (preset + Custom branches) and the inline-body/`f`-pin toggle are documented in `node_data/AGENTS.md` and `node_network/AGENTS.md` respectively. The apply placard (`apply_editor.dart`) renders a "wire a function into `f` to materialize argument pins" hint when `f` is disconnected, or a read-only summary of the wired source's signature when `f` is connected; argument pins themselves are emitted by the Rust post-pass (`update_apply_pin_layouts_for_network`). The map editor's `output_type` field switches to a read-only `_DerivedOutputTypeDisplay` (`map_editor.dart`) whenever `f` is connected; on disconnect, the field returns to its stored value. Body rendering is inherited from the zones UI — no closure-specific rendering code.
- **Add Node popup** is registry-driven, so `closure` and `apply` appear automatically once registered in Rust; no Flutter list edit was needed.
- **Closure ⇄ network conversion** (`doc/design_closure_network_conversion.md`): the node context menu (`node_network/node_widget.dart` `_handleContextMenu`) offers **"Convert to Closure"** on a custom-network instance used as a function, and **"Extract to Network..."** on a `closure` node — gated by the model's `canConvertInstanceToClosure` / `canExtractClosureToNetwork` (computed before `showMenu`). Convert is one-click (snackbar on error); Extract opens `extract_closure_to_network_dialog.dart` (name-only). Model methods `convertInstanceToClosure` / `extractClosureToNetwork` forward `scopeChain` and return a `ConversionResult { success, error }`.

## Structural Function / Iter types

`APIDataTypeBase` carries first-class `Iter` and `Function` variants alongside `Custom`, with one shared `children: List<APIDataType>` field on `APIDataType` whose meaning is interpreted locally to the base (`Iter` ⇒ 1 child: the element type; `Function` ⇒ N+1 children: params then return). Phase-1 commit migrated every existing literal site to `children: const []`; flat bases never use the field.

- **Authoring:** `DataTypeInput` (`lib/inputs/data_type_input.dart`) renders Iter / Function as a compact one-line summary + ✎ Edit button. The button opens `showTypeEditorDialog` (`lib/inputs/type_editor_dialog.dart`), a `DraggableDialog` hosting the full structural editor — `FunctionTypeInput` (`lib/inputs/function_type_input.dart`) for Function, a nested `DataTypeInput` for Iter. The summary is rendered by the top-level `apiDataTypeToString(APIDataType)` helper (e.g. `Iter[Float]`, `(Int, Bool) → String`, `Array[Iter[X]]`). The dialog has no Apply/Cancel — edits commit live; Ctrl+Z handles regret. Nested structural types stack dialogs naturally (Function returning Function = 2 levels).
- **Why a dialog, not inline:** inline editing got cramped fast — a `DataTypeInput` slot inside a per-row property editor is typically half the panel width, and the recursive Function widget bled visually past sibling rows with no scoping. Moving the structural editor to a dedicated draggable surface keeps the parent column thin at any nesting depth. Iter is dialog-hosted too (for consistency, even though one nested picker isn't itself cramped).
- **Default seeding:** the dropdown-change handler in `DataTypeInput` is the single point that seeds `children` — `Iter` ⇒ `[Float]`, `Function` ⇒ `[Float, Float]` (arity 1), everything else ⇒ `const []`. Switching away from Iter/Function drops `children` back to `const []`. The dialog and `FunctionTypeInput` defensively fall back to `Float` if `children` is malformed.
- **`FunctionTypeInput`** is closure-agnostic by design — function types have no parameter names (per the load-bearing invariant in `doc/design_custom_closure_kind.md`). The closure editor's `_CustomParamRow` is a separate widget that *additionally* carries a name field.
- **Custom… text fallback** stays for the long tail (anonymous records and any future types without first-class API surface). The Rust→API converter prefers the structural variants, so previously-typed `Custom: Iter[Int]` UI silently upgrades to the structural form on next paint — no migration needed.
- **Array semantics:** the outer array checkbox preserves `children`, so `Array[Iter[T]]` stays well-formed across toggles.

Design doc: `doc/design_structural_function_and_iter_types.md`. Per-editor smoke walkthrough is in `node_data/AGENTS.md`.

## Undo/Redo Integration

Keyboard shortcuts in `node_network/node_network.dart`:
- `Ctrl+Z` → `sd_api.undo()` + refresh
- `Ctrl+Shift+Z` / `Ctrl+Y` → `sd_api.redo()` + refresh

Drag coalescing in `node_network/node_widget.dart`:
- `sd_api.beginMoveNodes()` on drag start
- `sd_api.endMoveNodes()` on drag end
- Intermediate `moveSelectedNodes()` calls don't create undo commands

Model methods: `StructureDesignerModel.beginMoveNodes()` / `endMoveNodes()`.

## Execute action & Console panel

Right-click a node → **Execute** triggers a one-shot evaluation pass on that node with the side-effect flag set, gating effect nodes (`export_xyz`, `foreach`, `print` with `execute_only`) to actually fire. The Flutter side runs the FFI synchronously — `frb(sync)` — because `CAD_INSTANCE` has no internal synchronization and the persistent per-frame `provide_texture` callback would race against a worker-thread Rust call (see `doc/design_node_execution.md` "Why not async (worker thread) FFI"). To give the user feedback while the call blocks, the model method follows this recipe:

1. Show a non-dismissable `DraggableDialog` placard ("Executing…") with `barrierDismissible: false` and `dismissible: false`.
2. `await SchedulerBinding.instance.endOfFrame` so the dialog frame actually paints before the sync FFI takes over the UI thread.
3. Run `sd_api.executeNode(...)` inside `try { … } finally { Navigator.of(context).pop(); }` — the `finally` ensures the placard always dismisses, including on a thrown FFI error or a Rust panic surfaced through FRB.
4. After dismissal, surface success/error via the existing snackbar/status-message mechanism.

The placard intentionally uses a **static** icon (`Icons.hourglass_empty`), not a `CircularProgressIndicator` — the UI thread is blocked during the FFI call so any animated widget would freeze mid-frame and look broken.

The **Console panel** (`console_panel.dart`) is a docked-bottom strip showing entries pushed by `print` nodes. State lives on `StructureDesignerModel`:
- `printLog: List<APIPrintLogEntry>` accumulates entries.
- `consolePanelVisible: bool` toggles visibility (zero height when off).
- `unreadPrintLogCount: int` drives the new-entries dot.

`refreshFromKernel()` polls `sd_api.takePrintLog()` after every refresh and appends to `printLog` (drain-on-read keeps the Rust-side buffer bounded as long as the user occasionally exercises the app). The panel is wired into `structure_designer.dart` as the last child of the main `Column` (so it docks below the main content area), with a *View > Show/Hide Console* menu entry and a global **Ctrl + backtick** keyboard shortcut in `_handleGlobalKeyEvent`.

`APIExecuteResult.logs` carries only this Execute pass's prints (sliced Rust-side from `pass_start`); the model's polling drain handles general-case feeding, so the executeNode() call site does **not** also push `result.logs` into `printLog` — doing so would double-display the execute-pass entries.

**PlatformInt64 gotcha.** `APIPrintLogEntry.timestampMs` is FRB's `PlatformInt64` — typedef'd to `int` on native and `BigInt` on web. Console panel code uses `int` directly which is correct for desktop (the project's primary target); needs a `.toInt()` adapter if web ever becomes a real target.

Design doc: `doc/design_node_execution.md`.

## node_networks_list/ Subdirectory

Unified user-types panel — lists both node networks and record type defs:
- `node_networks_panel.dart` - Tab container (List/Tree views) + action bar
- `node_network_list_view.dart` - Flat list with rename, validation error indicators; shows kind icon (network vs record def)
- `node_network_tree_view.dart` - Hierarchical tree view (networks + record defs in one tree)
- `node_networks_action_bar.dart` - Add/delete/navigate buttons; the "Add" action offers both "new network" and "new record def"

Selecting an entry sets it active in the model — networks set `activeNetworkName`, record defs set `activeRecordDefName`. `MainContentArea` swaps the editor accordingly.
