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

Keyboard shortcuts: hold **B** for spring-loaded AddBond tool activation (deferred release during active drag); **1-7** set bond order in AddBond tool or change selected bond(s) order in Default tool.

Design doc: `doc/atom_edit/design_bond_creation_and_order.md`.

## node_networks_list/ Subdirectory

Network management panel with:
- `node_networks_panel.dart` - Tab container (List/Tree views) + action bar
- `node_network_list_view.dart` - Flat list with rename, validation error indicators
- `node_network_tree_view.dart` - Hierarchical tree view
- `node_networks_action_bar.dart` - Add/delete/navigate buttons
