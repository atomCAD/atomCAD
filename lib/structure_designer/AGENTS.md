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
├── display_panel.dart                # DISPLAY section: assembles the clusters below
├── display_button_group.dart         # Icon button + grouping/separator/wrap rules
├── geometry_visualization_widget.dart          # Geometry 3D display cluster
├── atomic_structure_visualization_widget.dart  # Atom/bond 3D display cluster
├── background_visualization_widget.dart        # Show axes / show grid cluster
├── node_display_widget.dart          # Display policy cluster (Manual/Selected/Frontier)
├── mode_toggle_widget.dart           # Direct Editing / Node Network mode cluster
├── preferences_window.dart           # Settings dialog
├── factor_into_subnetwork_dialog.dart # Extract selection to subnetwork
├── extract_closure_to_network_dialog.dart # Name dialog for Closure→Network conversion
├── import_cnnd_library_dialog.dart   # Import from .cnnd library
├── identifier_validation.dart        # Field/identifier validation rules
├── namespace_utils.dart              # User-type-name validation (networks + record defs share one namespace)
├── qualified_name_header.dart        # Qualified-name header strip (breaks after the namespace only when too long) + copy button (#207/#307)
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

## DISPLAY panel (sidebar)

The DISPLAY section is a bar of small icon buttons assembled by `display_panel.dart` out of *clusters*, one per subject, each built by the file that owns that subject (`geometry_visualization_widget.dart`, `background_visualization_widget.dart`, …). Those files export a `xxxCluster(model)` **function**, not a widget — `DisplayPanel` owns the single `Consumer<StructureDesignerModel>`.

The grouping contract is documented in `display_button_group.dart` and is worth knowing before adding a control: a `DisplayButtonGroup` is never a mix of radio-group members and toggles (both render "selected" identically, so a mixed group is ambiguous), and separators between groups are inserted by `DisplayGroupBar` rather than by callers. Line breaking is computed from a fixed button extent — if you change `DisplayIconButton`'s icon size or padding, update the extent constants alongside it.

**Adding a display control means adding it to the cluster whose subject it belongs to.** A new cluster is warranted only by a genuinely new subject, and must then be added to *both* branches of `DisplayPanel` (or deliberately just the Node Network one — Direct Editing Mode drops the geometry and node-display-policy clusters).

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

A **guideline** is a transient line that constrains atom placement to hard-to-hit positions (issue #368): a dedicated **fourth tool** in the atom_edit toolbar (`Icons.timeline`, F5), gated to `atom_edit` and not offered on `motif`. It is **not serialized and not undoable**, and vanishes on tool switch or node deselect.

The three-phase panel, the model methods, and the viewport delegate are documented at `node_data/atom_edit_editor.dart::_buildGuidelinePanel`. Design doc: `doc/atom_edit/design_atom_guidelines.md`.

## Click-to-Activate (in viewport)

Clicking a non-active node's rendered output activates that node (two-step: first click activates, second performs the normal action), via a `viewport_pick()` interception in `onPointerDown` before delegate dispatch. Overlapping outputs raise a disambiguation overlay.

The mechanism, the **scroll-to-node callback pattern** that bridges viewport → model → node-network-widget, and the `scopeChain` / `screenAnchor` extras Find Usages reuses are documented in `structure_designer_viewport.dart`'s library doc.

## Find Usages (issue #414)

The inverse of *Go to Definition*: from a custom-node instance, jump to the other places its type is used. Design doc: `doc/design_find_usages.md`.

- **Backend (Phase 1)** owns the walk and the display strings: `sd_api.getNetworkUsages(networkName:)` → `List<APINetworkUsage> { hostNetwork, scopePath, nodeId, nodeLabel, bodyQualifier }`, plus a batched `getNetworkUsageCounts()` for the panel. Read-only — no refresh, no undo. Flutter never re-derives a label or a body qualifier from these.
- **Entry point (Phase 2)** is the node context menu (`node_network/node_widget.dart` `_handleContextMenu` → `_handleFindUsages`), gated on `isCustomNodeType`. It **drops the originating instance** (active network + clicked node's scope chain + node id) — the backend deliberately returns the unfiltered set so the panel entry points (Phase 3) can reuse it. On the filtered set: 0 → SnackBar "No other usages of …", 1 → jump straight away, 2+ → a `showMenu` picker at the cursor.
- **The 0/1/n branching and the picker are shared** — `find_usages_menu.dart` (`showNetworkUsagesMenu`, `networkUsageLabel`, `menuPositionForWidget`), which documents its own contract. Only what genuinely differs stays at the call site: the self-filter and the empty-case wording. A new entry point should call it, not re-implement the branches.
- **Entry points (Phase 3)** are the user-types panel: a *Find Usages* row context-menu item in **both** `node_network_list_view.dart` and `node_network_tree_view.dart` (network rows only — record defs have no usage search), plus a trailing usage count in the list view. Both go through `findUsagesOfNetwork`, which queries the **unfiltered** set and passes no `screenAnchor`, so the landing is viewport-centered.
- **The jump** is `StructureDesignerModel.jumpToUsage`; the count comes from `StructureDesignerModel.networkUsageCounts`, not from a per-row query. Both carry their own doc comments (including the "look up by name, never iterate" rule), as does the landing logic in `node_network.dart::_scrollToNode` — read those before changing the jump behavior.

Design doc: `doc/design_click_to_activate_node.md`.

## Navigation Up-Axis (view-up picker, issue #349)

The turntable's screen-vertical axis is pickable (default world +Z). Three pieces:

- **Orbit math** in `lib/common/cad_viewport.dart::rotateCamera`, which reads the axis from `camera.navUp` (on the extended `APICamera`). **Not unit-tested** — verify by manual walkthrough (`doc/design_view_up_axis.md` Verification).
- **Camera-row control** (`camera_control_widget.dart` `_buildUpAxisButton`): an "Up: ⟨label⟩" `TextButton` after the ortho toggle, highlighted when the axis is non-default. The label is `ConstrainedBox(maxWidth: 90)` + ellipsis so a long index like `[10 10 10]` can't overflow the sidebar next to the `Expanded` canonical-view dropdown.
- **Dialog** (`view_up_axis_dialog.dart`, `showViewUpAxisDialog`) — its library doc carries the details, including the `_adoptFromLabel` trap.

Model side (`structure_designer_model.dart`): `viewUpInfo` (`APIViewUpInfo?`, mirrored each `refreshFromKernel`), the `setViewUpFromMillerPlane` / `setViewUpFromLatticeDirection` / `setViewUpFromActiveDrawingPlane` wrappers (each returns the kernel's error string, surfaced inline) and `resetViewUp`. No `scope_path` — camera is global to the active network, like the other camera methods. Design doc: `doc/design_view_up_axis.md`.

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
- **`toggleOutputPinDisplay(nodeId, pinIndex, {scopeChain})`** model method toggles individual pin visibility. Pass the node's `scopeChain` — a body node's id collides with a top-level one, and body pins are togglable inside a parameter-less closure (see `node_network/AGENTS.md` → "Body-node visibility eyes"). Same for `toggleNodeDisplay`, which resolves the node through `_resolveNodeInScope` rather than a bare `nodes[nodeId]` lookup.
- **Wire rendering:** output pin y-offset is per-pin (same formula as input pins). `getNodeSize()` / `estimate_node_height()` use `max(inputs, outputs, minHeight)`.
- **`OutputPinView { name, data_type, index }`** API type for each output pin.

## Closures (function values)

The `closure` and `apply` nodes (plus the four HOFs' optional `f` input pin) expose first-class function values to the UI. See `doc/design_closures.md`, `doc/design_currying.md`, and `doc/design_function_pin_unification.md`.

- **API types:** `APIClosureData` / `APIApplyData` (both `{ kind, type_args, param_names }`; the `APIClosureKind` variants mirror Rust's — see `rust/src/structure_designer/nodes/AGENTS.md`). `Function` **and** `AnyFunction` pins render amber, the latter with a node-specific tooltip line (apply: "apply will call it on the wired arguments"; map: "applied per element of the stream"). **`APIDerivedShapeView.derived_from_input_pin`** on `NodeView` is `Some("f")` exactly when the node's layout/output type is currently derived from a wired `f` — it drives the apply placard and the map output-type display switch.
- **Model / API:** `setClosureData` / `setApplyData` are model methods (forwarding `activeScopeChain` as `scope_path`); `getClosureData` / `getApplyData` are direct generated-API calls. `node_data_widget.dart` routes `'closure'` to the shared `ClosureShapeEditor` and `'apply'` to `apply_editor.dart`. Both editors, plus the inline-body/`f`-pin toggle, are documented in `node_data/AGENTS.md` and `node_network/AGENTS.md`. Body rendering is inherited from the zones UI — there is no closure-specific rendering code, and the Add Node popup is registry-driven, so no Flutter list edit was needed for either node.
- **Closure ⇄ network conversion** (`doc/design_closure_network_conversion.md`): the node context menu (`node_network/node_widget.dart` `_handleContextMenu`) offers **"Convert to Closure"** on a custom-network instance used as a function, and **"Extract to Network..."** on a `closure` node — gated by the model's `canConvertInstanceToClosure` / `canExtractClosureToNetwork` (computed before `showMenu`). Convert is one-click (snackbar on error); Extract opens `extract_closure_to_network_dialog.dart` (name-only). Model methods `convertInstanceToClosure` / `extractClosureToNetwork` forward `scopeChain` and return a `ConversionResult { success, error }`.

## Function pin roles (the "Function output" sidebar section)

Any node can be used as a function value through its title-bar `-1` pin; which of its inputs stay parameters and which are baked in is overridable per pin (`doc/design_function_pin_roles.md`, issue #408). The UI is one generic section — `node_data/function_output_editor.dart`, rendered by `node_data_widget.dart` for **every** selected node rather than switched on a node type; see `node_data/AGENTS.md` for its contents and the two filtered-out pin sets.

- **API:** `getFunctionPinRoles(scopePath, nodeId)` → `List<APIFunctionPinRoleView { pinName, role, wired, effective }>` (a direct generated-API call) and `model.setFunctionPinRole(nodeId, pinIndex, role)` (forwards `propertyEditorScopeChain`). The getter reports `Auto` explicitly even though Rust stores it as absence — Flutter never needs the absence-is-Auto convention.
- **`effective` is rendered verbatim, never re-derived** (it comes from the shared Rust `function_pin_dispositions` helper), and **a function-mode node is an ordinary node for display purposes** — Flutter uses `NodeView.function_pin_consumed` only to expand this section by default, never to gate the eye icon.

## Structural Function / Iter types

`APIDataTypeBase` carries first-class `Iter` and `Function` variants alongside `Custom`, sharing one `children: List<APIDataType>` field whose meaning is interpreted locally to the base. The authoring surface is `DataTypeInput` (`lib/inputs/data_type_input.dart`) — its library doc carries the `children` contract, the default-seeding rule, the `Optional[T]` gating, and why the structural editor is a dialog rather than inline. The dialog itself is `showTypeEditorDialog` (`lib/inputs/type_editor_dialog.dart`), hosting `FunctionTypeInput` for Function and a nested `DataTypeInput` for Iter; it has no Apply/Cancel (edits commit live, Ctrl+Z handles regret) and nested structural types stack dialogs naturally.

One invariant worth stating outside that file: **`FunctionTypeInput` is closure-agnostic by design** — function types have no parameter names (the load-bearing invariant in `doc/design_custom_closure_kind.md`). The closure editor's `_CustomParamRow` is a *separate* widget that additionally carries a name field; do not merge them.

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

Right-click a node → **Execute** triggers a one-shot evaluation pass on that node with the side-effect flag set, gating effect nodes (`export_atoms`, `foreach`, `print` with `execute_only`) to actually fire.

**The FFI runs synchronously (`frb(sync)`)** because `CAD_INSTANCE` has no internal synchronization and the persistent per-frame `provide_texture` callback would race a worker-thread Rust call (`doc/design_node_execution.md`, "Why not async (worker thread) FFI"). That blocks the UI thread, so the call goes through **`runExecuteWithPlacard`** (`node_network/node_widget.dart`), whose doc comment carries the required modal-placard recipe — follow it for any future blocking sync-FFI action rather than inventing a second one.

The **Console panel** (`console_panel.dart`) is a docked-bottom strip showing entries pushed by `print` nodes; its state lives on `StructureDesignerModel` (`printLog`, `consolePanelVisible`, `unreadPrintLogCount`) and `refreshFromKernel()` polls `sd_api.takePrintLog()`. The panel's own library doc covers the drain model, the "don't also push `APIExecuteResult.logs`" trap, and the `PlatformInt64` gotcha.

Design doc: `doc/design_node_execution.md`.

## node_networks_list/ Subdirectory

Unified user-types panel — lists both node networks and record type defs:
- `node_networks_panel.dart` - Tab container (List/Tree views) + action bar
- `node_network_list_view.dart` - Flat list with rename, validation error indicators; shows kind icon (network vs record def)
- `node_network_tree_view.dart` - Hierarchical tree view (networks + record defs in one tree)
- `node_networks_action_bar.dart` - Add/delete/navigate buttons; the "Add" action offers both "new network" and "new record def"

Selecting an entry sets it active in the model — networks set `activeNetworkName`, record defs set `activeRecordDefName`. `MainContentArea` swaps the editor accordingly.

**Creation is namespace-relative** (issue #308). A new user type lands in `StructureDesignerModel.activeNamespace` — the folder of the active record def, else of the active network, else the root — never unconditionally at the root. Any *new* creation entry point must follow this: pass a namespace to `addNewNodeNetworkInNamespace` / `addNewRecordTypeDefInNamespace` / `addFolder` rather than calling a root-scoped variant, and name the destination in the tooltip or dialog subtitle so the user can see where it will go. The tree view has no selection state of its own, so "selected" and "active" are the same thing here.

The corollary is load-bearing: because the buttons inherit, **the root needs its own explicit gesture** or it becomes unreachable in a fully namespaced design. That is the tree view's background right-click menu (`_withRootContextMenu` / `_showRootContextMenu`). Don't remove it while the action bar inherits, and if you add a third creation surface, decide which of the two it is.
