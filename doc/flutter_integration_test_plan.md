# Flutter Integration Test Plan

This document outlines a phased approach to expanding Flutter integration test coverage for atomCAD.

## Current State

Existing tests in `integration_test/`:
- **simple_test.dart**: FFI initialization only
- **app_test.dart**: App launch, menu visibility, sidebar sections, tab switching, add network button
- **node_network_test.dart**: Add/delete networks, confirmation dialogs, screenshots

## Proposed File Structure

```
integration_test/
├── smoke_test.dart                    # Basic app launch + FFI
├── menu_test.dart                     # All menu interactions
├── node_network/
│   ├── network_list_test.dart         # Add/delete/rename networks
│   ├── node_operations_test.dart      # Add/select/delete nodes
│   └── keyboard_shortcuts_test.dart   # Delete key, Ctrl+D, etc.
├── panels/
│   ├── display_panel_test.dart        # Geometry/atomic visualization
│   ├── camera_panel_test.dart         # Camera controls
│   └── properties_panel_test.dart     # Node property editors
├── dialogs/
│   ├── preferences_test.dart          # Preferences window
│   └── add_node_popup_test.dart       # Add node filtering/selection
└── helpers/
    └── test_utils.dart                # Shared pumpApp, key constants
```

## Skipped Functionality

The following features are intentionally skipped due to testing complexity:

| Feature | Reason |
|---------|--------|
| **Wire connection (drag from pin to pin)** | Requires precise coordinate calculations for pin positions, bezier hit testing |
| **Wire selection** | Requires clicking on bezier curves with precise coordinates |
| **Rectangle multi-select** | Complex drag operation with coordinate math |
| **Node dragging/repositioning** | Gesture simulation complexity |
| **3D Viewport interactions** | GPU rendering, mouse gestures on wgpu surface |
| **File save/load end-to-end** | File system interaction; we test that dialogs appear instead |
| **Import from .cnnd library full flow** | Multi-step dialog with file picker |

---

## Phase 1: Foundation & Refactoring ✅ COMPLETED

**Goal**: Establish testing patterns, shared utilities, and reorganize existing tests.

### Tasks

- [x] Create `integration_test/helpers/test_utils.dart` with:
  - Shared `pumpApp()` helper function
  - Key constants class (`TestKeys`)
  - Common finder helpers

- [x] Refactor existing tests:
  - Move FFI test from `simple_test.dart` to `smoke_test.dart`
  - Extract menu tests from `app_test.dart` to `menu_test.dart`
  - Move network list tests from `node_network_test.dart` to `node_network/network_list_test.dart`

- [x] Add Keys to source code:
  - `structure_designer.dart`: Menu items (`fileMenu`, `viewMenu`, `editMenu`)
  - `node_networks_panel.dart`: Panel elements (`addNetworkButton`, `deleteNetworkButton`, `networkListTab`, `networkTreeTab`)

### Tests to Write

| Test | File | Status |
|------|------|--------|
| App launches successfully | `smoke_test.dart` | ✅ |
| FFI initialization works | `smoke_test.dart` | ✅ |
| File menu opens | `menu_test.dart` | ✅ |
| View menu opens | `menu_test.dart` | ✅ |
| Edit menu opens | `menu_test.dart` | ✅ |
| Load Design dialog appears | `menu_test.dart` | ✅ |
| Save Design As dialog appears | `menu_test.dart` | ✅ |
| Export visible dialog appears | `menu_test.dart` | ✅ |

---

## Phase 2: Node Networks Panel ✅ COMPLETED

**Goal**: Test network management operations.

### Tasks

- [x] Add Keys to source code:
  - Network list items (dynamic keys per network name): `network_item_$networkName`
  - Tree view items: `network_tree_item_$networkName`, `namespace_tree_item_$namespacePath`
  - Rename text field: `rename_text_field`
  - Delete confirmation dialog: `delete_confirm_dialog`

### Tests Written

| Test | File | Status |
|------|------|--------|
| Add network button creates network | `network_list_test.dart` | ✅ |
| Add multiple networks and verify count | `network_list_test.dart` | ✅ |
| Delete network shows confirmation dialog | `network_list_test.dart` | ✅ |
| Confirm delete removes network | `network_list_test.dart` | ✅ |
| Cancel delete keeps network | `network_list_test.dart` | ✅ |
| Switch between List and Tree tabs | `network_list_test.dart` | ✅ |
| Model setActiveNodeNetwork works correctly | `network_list_test.dart` | ✅ |
| Network list items are displayed with correct Keys | `network_list_test.dart` | ✅ |
| Network tree view displays networks | `network_list_test.dart` | ✅ |
| Back and forward buttons exist | `network_list_test.dart` | ✅ |
| Back button navigates to previous network | `network_list_test.dart` | ✅ |
| Forward button navigates to next network | `network_list_test.dart` | ✅ |
| Selecting network clears forward history | `network_list_test.dart` | ✅ |

### Notes

- The tree view uses `AnimatedTreeView` which virtualizes off-screen items, making specific Key-based item finding unreliable when there are many networks
- Tests use the model's `setActiveNodeNetwork()` method directly for selection to ensure reliable testing
- Navigation history tests verify relative behavior (navigated to a different network) rather than absolute names due to accumulated state across tests

---

## Phase 3: Display & Camera Panels

**Goal**: Test sidebar panel controls.

### Tasks

- [ ] Add Keys to source code:
  - `geometry_visualization_widget.dart`: Dropdown key
  - `node_display_widget.dart`: Dropdown key
  - `atomic_structure_visualization_widget.dart`: Dropdown key
  - `camera_control_widget.dart`: Control elements

### Tests to Write

| Test | File | Description |
|------|------|-------------|
| Geometry visualization dropdown works | `display_panel_test.dart` | Change mode, verify selection |
| Node display policy dropdown works | `display_panel_test.dart` | Change policy, verify selection |
| Atomic visualization dropdown works | `display_panel_test.dart` | Change mode, verify selection |
| Camera control panel visible | `camera_panel_test.dart` | Verify panel and controls exist |

---

## Phase 4: Preferences Dialog

**Goal**: Test preferences window functionality.

### Tasks

- [ ] Add Keys to source code:
  - `preferences_window.dart`: All dropdowns, checkboxes, inputs

### Tests to Write

| Test | File | Description |
|------|------|-------------|
| Preferences opens from Edit menu | `preferences_test.dart` | Edit > Preferences shows dialog |
| Preferences closes on X button | `preferences_test.dart` | Click close, dialog dismissed |
| Visualization method dropdown works | `preferences_test.dart` | Change selection |
| Display camera pivot checkbox works | `preferences_test.dart` | Toggle checkbox |
| Background color inputs work | `preferences_test.dart` | Change RGB values |
| Show grid checkbox works | `preferences_test.dart` | Toggle checkbox |

---

## Phase 5: Add Node Popup

**Goal**: Test node creation dialog.

### Tasks

- [ ] Add Keys to source code:
  - `add_node_popup.dart`: Dialog, filter field, list items, description panel

### Tests to Write

| Test | File | Description |
|------|------|-------------|
| Add node popup opens on right-click | `add_node_popup_test.dart` | Right-click canvas, verify dialog |
| Filter field filters node list | `add_node_popup_test.dart` | Type "cube", verify filtered results |
| Selecting node closes popup | `add_node_popup_test.dart` | Click node type, verify dialog closes |
| Hovering shows description | `add_node_popup_test.dart` | Hover node, verify description panel |
| Categories are displayed | `add_node_popup_test.dart` | Verify category headers visible |

---

## Phase 6: Node Operations

**Goal**: Test basic node interactions in the network editor.

### Tasks

- [ ] Add Keys to source code:
  - `node_widget.dart`: Node container, visibility button, pins
  - `node_network.dart`: Canvas key

### Tests to Write

| Test | File | Description |
|------|------|-------------|
| Create node via popup | `node_operations_test.dart` | Right-click, select type, verify node appears |
| Select node by clicking | `node_operations_test.dart` | Click node, verify selected state |
| Delete key removes selected node | `keyboard_shortcuts_test.dart` | Select node, press Delete |
| Backspace removes selected node | `keyboard_shortcuts_test.dart` | Select node, press Backspace |
| Ctrl+D duplicates node | `keyboard_shortcuts_test.dart` | Select node, Ctrl+D, verify duplicate |
| Toggle node visibility | `node_operations_test.dart` | Click eye icon, verify visibility changes |
| Click empty space clears selection | `node_operations_test.dart` | Select node, click empty, verify deselected |

---

## Phase 7: Node Properties Panel

**Goal**: Test property editors for various node types.

### Tasks

- [ ] Add Keys to source code:
  - `node_data_widget.dart`: Editor container
  - Individual editors: Input fields, checkboxes, dropdowns

### Tests to Write

| Test | File | Description |
|------|------|-------------|
| Properties panel shows for selected node | `properties_panel_test.dart` | Select node, verify editor appears |
| Int editor accepts valid input | `properties_panel_test.dart` | Create int node, change value |
| Float editor accepts valid input | `properties_panel_test.dart` | Create float node, change value |
| Bool editor toggles | `properties_panel_test.dart` | Create bool node, toggle checkbox |
| String editor accepts text | `properties_panel_test.dart` | Create string node, enter text |
| Vec3 editor accepts values | `properties_panel_test.dart` | Create vec3 node, change x/y/z |
| Cuboid editor shows all fields | `properties_panel_test.dart` | Create cuboid, verify min/extent fields |
| Sphere editor shows all fields | `properties_panel_test.dart` | Create sphere, verify center/radius fields |

---

## Keys Reference

Suggested key constants to add (in `test_utils.dart` or source files):

```dart
// Menu keys
static const Key fileMenu = Key('file_menu');
static const Key viewMenu = Key('view_menu');
static const Key editMenu = Key('edit_menu');
static const Key loadDesignMenuItem = Key('load_design_item');
static const Key saveDesignMenuItem = Key('save_design_item');
static const Key saveDesignAsMenuItem = Key('save_design_as_item');
static const Key exportVisibleMenuItem = Key('export_visible_item');
static const Key preferencesMenuItem = Key('preferences_item');

// Node networks panel keys
static const Key nodeNetworksPanel = Key('node_networks_panel');
static const Key networkListTab = Key('network_list_tab');
static const Key networkTreeTab = Key('network_tree_tab');
static const Key addNetworkButton = Key('add_network_button');
static const Key deleteNetworkButton = Key('delete_network_button');
static const Key backButton = Key('back_button');
static const Key forwardButton = Key('forward_button');

// Network list items (dynamic keys) - Phase 2
static Key networkListItem(String networkName) => Key('network_item_$networkName');
static Key networkTreeItem(String networkName) => Key('network_tree_item_$networkName');
static Key namespaceTreeItem(String namespacePath) => Key('namespace_tree_item_$namespacePath');
static const Key renameTextField = Key('rename_text_field');

// Display panel keys
static const Key geometryVisualizationDropdown = Key('geometry_vis_dropdown');
static const Key nodeDisplayDropdown = Key('node_display_dropdown');
static const Key atomicVisualizationDropdown = Key('atomic_vis_dropdown');

// Add node popup keys
static const Key addNodeDialog = Key('add_node_dialog');
static const Key addNodeFilterField = Key('add_node_filter_field');
static const Key addNodeListView = Key('add_node_list_view');

// Preferences window keys
static const Key preferencesDialog = Key('preferences_dialog');
static const Key preferencesCloseButton = Key('preferences_close_button');
static const Key visualizationMethodDropdown = Key('vis_method_dropdown');
static const Key displayCameraTargetCheckbox = Key('display_camera_target_cb');
static const Key showGridCheckbox = Key('show_grid_cb');

// Node widget keys (dynamic)
Key nodeWidget(BigInt id) => Key('node_$id');
Key nodeVisibilityButton(BigInt id) => Key('node_visibility_$id');

// Node network canvas
static const Key nodeNetworkCanvas = Key('node_network_canvas');
```

---

## Success Criteria

Each phase is complete when:
1. All listed tests pass
2. Keys are added to source code
3. Tests use Keys instead of text finders where possible
4. No flaky tests (run 3x without failure)

## Notes

- Tests should be independent and not rely on state from other tests
- Each test file should have its own `setUp`/`tearDown` for fresh model state
- Use `pumpAndSettle()` after interactions that trigger animations
- Screenshots can be captured for visual regression testing but are optional
