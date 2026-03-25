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
├── main_content_area.dart            # Resizable split: viewport + node editor
├── camera_control_widget.dart        # Camera view selector (ortho/perspective)
├── node_display_widget.dart          # Display policy buttons (Manual/Selected/Frontier)
├── atomic_structure_visualization_widget.dart  # Atom/bond 3D display
├── geometry_visualization_widget.dart          # Geometry 3D display
├── preferences_window.dart           # Settings dialog
├── factor_into_subnetwork_dialog.dart # Extract selection to subnetwork
├── import_cnnd_library_dialog.dart   # Import from .cnnd library
├── namespace_utils.dart              # Network name validation
├── node_network/                     # Node graph editor
├── node_data/                        # Per-node-type property editors
└── node_networks_list/               # Network list/tree panels
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

## Click-to-Activate (in viewport)

When multiple nodes are visible, clicking on a non-active node's rendered output in the 3D viewport activates that node (two-step interaction: first click activates, second click performs the normal action). The interception happens in `onPointerDown` before delegate dispatch, calling `viewport_pick()` (Rust API) which returns `ActivateNode`, `Disambiguation`, `ActiveNodeHit`, or `NoHit`. A performance guard skips the pick when only 0–1 nodes are displayed.

When overlapping outputs are detected (within 0.1 Å), a disambiguation overlay popup (`_DisambiguationOverlay`) appears near the click with two actions per candidate node: name click (activate + scroll) and solo eye icon (activate + scroll + hide other overlapping nodes). If the active node is among the overlapping hits, the click passes through as normal.

**Scroll-to-node callback pattern:** The viewport calls `model.scrollToNode(nodeId)` after activation. `StructureDesignerModel.onScrollToNode` is a callback registered by `NodeNetworkState` during `initState` (and cleared in `dispose`). This bridges the viewport→model→node-network-widget communication without requiring the viewport to hold a `GlobalKey` to the node network. SnackBar feedback (`"Activated: {nodeName}"`) confirms the activation.

Design doc: `doc/design_click_to_activate_node.md`.

## Multi-Output Pin UI

- **Eye icon** is per output pin, not in the title bar. Each output pin row has its own eye toggle.
- **Multi-output nodes** (e.g. atom_edit) show pin names ("result", "diff") next to each output pin. Single-output nodes do not show pin names.
- **`NodeView.output_pins: Vec<OutputPinView>`** and **`displayed_pins: Vec<i32>`** from the Rust API.
- **`toggleOutputPinDisplay(nodeId, pinIndex)`** model method toggles individual pin visibility.
- **Wire rendering:** output pin y-offset is per-pin (same formula as input pins). `getNodeSize()` / `estimate_node_height()` use `max(inputs, outputs, minHeight)`.
- **`OutputPinView { name, data_type, index }`** API type for each output pin.

## Undo/Redo Integration

Keyboard shortcuts in `node_network/node_network.dart`:
- `Ctrl+Z` → `sd_api.undo()` + refresh
- `Ctrl+Shift+Z` / `Ctrl+Y` → `sd_api.redo()` + refresh

Drag coalescing in `node_network/node_widget.dart`:
- `sd_api.beginMoveNodes()` on drag start
- `sd_api.endMoveNodes()` on drag end
- Intermediate `moveSelectedNodes()` calls don't create undo commands

Model methods: `StructureDesignerModel.beginMoveNodes()` / `endMoveNodes()`.

## node_networks_list/ Subdirectory

Network management panel with:
- `node_networks_panel.dart` - Tab container (List/Tree views) + action bar
- `node_network_list_view.dart` - Flat list with rename, validation error indicators
- `node_network_tree_view.dart` - Hierarchical tree view
- `node_networks_action_bar.dart` - Add/delete/navigate buttons
