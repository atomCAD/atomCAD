# Multi-Node Selection Implementation Plan

## Overview

This document outlines the implementation plan for adding multi-node selection and group movement to atomCAD's node network editor.

## Current Architecture

### Selection State (Rust Backend)

**File:** [`rust/src/structure_designer/node_network.rs`](file:///c:/machine_phase_systems/flutter_cad/rust/src/structure_designer/node_network.rs#L140-L150)

```rust
pub struct NodeNetwork {
  // ...
  pub selected_node_id: Option<u64>,    // Single selected node
  pub selected_wire: Option<Wire>,       // Single selected wire
  // ...
}
```

Key methods:
- `select_node(node_id)` - Sets `selected_node_id`, clears `selected_wire`
- `select_wire(...)` - Sets `selected_wire`, clears `selected_node_id`
- `clear_selection()` - Clears both
- `delete_selected()` - Deletes selected node or wire
- `provide_gadget()` - Returns gadget for selected node

### Selection State (Flutter Frontend)

**File:** [`lib/structure_designer/structure_designer_model.dart`](file:///c:/machine_phase_systems/flutter_cad/lib/structure_designer/structure_designer_model.dart#L268-L287)

- `setSelectedNode(BigInt nodeId)` - Calls Rust API
- `getSelectedNodeId()` - Iterates nodes to find selected one
- `clearSelection()` - Calls Rust API

### UI Selection Handling

**File:** [`lib/structure_designer/node_network/node_widget.dart`](file:///c:/machine_phase_systems/flutter_cad/lib/structure_designer/node_network/node_widget.dart#L237-L262)

- `onTapDown` / `onPanStart` → `model.setSelectedNode(node.id)`
- Node decoration uses `node.selected` boolean for styling

**File:** [`lib/structure_designer/node_network/node_network.dart`](file:///c:/machine_phase_systems/flutter_cad/lib/structure_designer/node_network/node_network.dart#L193-L208)

- `_handleWireTap()` - Selects wire or clears selection on empty space click

### Node Movement

**File:** [`lib/structure_designer/structure_designer_model.dart`](file:///c:/machine_phase_systems/flutter_cad/lib/structure_designer/structure_designer_model.dart#L182-L201)

- `dragNodePosition(BigInt nodeId, Offset delta)` - UI-only position update during drag
- `updateNodePosition(BigInt nodeId)` - Commits position to Rust kernel

---

## Implementation Plan

### Phase 1: Extend Selection State in Rust Backend

#### 1.1 Modify `NodeNetwork` struct

**File:** `rust/src/structure_designer/node_network.rs`

```rust
pub struct NodeNetwork {
  // Replace:
  // pub selected_node_id: Option<u64>,
  // pub selected_wire: Option<Wire>,
  
  // With:
  pub selected_node_ids: HashSet<u64>,   // All selected nodes
  pub active_node_id: Option<u64>,       // Active node (for properties/gadget)
  pub selected_wires: Vec<Wire>,         // All selected wires
  // ...
}
```

#### 1.2 Add new selection methods

```rust
impl NodeNetwork {
  // ===== NODE SELECTION =====
  
  /// Select a single node (clears existing selection including wires)
  pub fn select_node(&mut self, node_id: u64) -> bool {
    if self.nodes.contains_key(&node_id) {
      self.selected_wires.clear();
      self.selected_node_ids.clear();
      self.selected_node_ids.insert(node_id);
      self.active_node_id = Some(node_id);
      true
    } else {
      false
    }
  }
  
  /// Toggle node in selection (for Ctrl+click)
  pub fn toggle_node_selection(&mut self, node_id: u64) -> bool {
    if !self.nodes.contains_key(&node_id) {
      return false;
    }
    self.selected_wires.clear();
    if self.selected_node_ids.contains(&node_id) {
      self.selected_node_ids.remove(&node_id);
      // Update active node if we removed it
      if self.active_node_id == Some(node_id) {
        self.active_node_id = self.selected_node_ids.iter().next().copied();
      }
    } else {
      self.selected_node_ids.insert(node_id);
      self.active_node_id = Some(node_id);
    }
    true
  }
  
  /// Add node to selection (for Shift+click)
  pub fn add_node_to_selection(&mut self, node_id: u64) -> bool {
    if !self.nodes.contains_key(&node_id) {
      return false;
    }
    self.selected_wires.clear();
    self.selected_node_ids.insert(node_id);
    self.active_node_id = Some(node_id);
    true
  }
  
  /// Select multiple nodes (for rectangle selection)
  pub fn select_nodes(&mut self, node_ids: Vec<u64>) -> bool {
    self.selected_wires.clear();
    self.selected_node_ids.clear();
    for id in &node_ids {
      if self.nodes.contains_key(id) {
        self.selected_node_ids.insert(*id);
      }
    }
    // Set active to last node in list (or none if empty)
    self.active_node_id = node_ids.last().copied()
      .filter(|id| self.selected_node_ids.contains(id));
    !self.selected_node_ids.is_empty()
  }
  
  /// Toggle multiple nodes in selection (for Ctrl+rectangle)
  pub fn toggle_nodes_selection(&mut self, node_ids: Vec<u64>) {
    self.selected_wires.clear();
    for id in node_ids {
      if self.nodes.contains_key(&id) {
        if self.selected_node_ids.contains(&id) {
          self.selected_node_ids.remove(&id);
        } else {
          self.selected_node_ids.insert(id);
          self.active_node_id = Some(id);
        }
      }
    }
    // Update active node if removed
    if let Some(active) = self.active_node_id {
      if !self.selected_node_ids.contains(&active) {
        self.active_node_id = self.selected_node_ids.iter().next().copied();
      }
    }
  }
  
  /// Check if a node is selected
  pub fn is_node_selected(&self, node_id: u64) -> bool {
    self.selected_node_ids.contains(&node_id)
  }
  
  /// Check if a node is the active node
  pub fn is_node_active(&self, node_id: u64) -> bool {
    self.active_node_id == Some(node_id)
  }
  
  /// Get all selected node IDs
  pub fn get_selected_node_ids(&self) -> &HashSet<u64> {
    &self.selected_node_ids
  }
  
  // ===== WIRE SELECTION =====
  
  /// Select a single wire (clears existing selection including nodes)
  pub fn select_wire(&mut self, wire: Wire) -> bool {
    if self.nodes.contains_key(&wire.source_node_id) 
       && self.nodes.contains_key(&wire.destination_node_id) {
      self.selected_node_ids.clear();
      self.active_node_id = None;
      self.selected_wires.clear();
      self.selected_wires.push(wire);
      true
    } else {
      false
    }
  }
  
  /// Toggle wire in selection (for Ctrl+click)
  pub fn toggle_wire_selection(&mut self, wire: Wire) -> bool {
    if !self.nodes.contains_key(&wire.source_node_id) 
       || !self.nodes.contains_key(&wire.destination_node_id) {
      return false;
    }
    self.selected_node_ids.clear();
    self.active_node_id = None;
    
    // Check if wire already selected
    if let Some(idx) = self.selected_wires.iter().position(|w| w.eq(&wire)) {
      self.selected_wires.remove(idx);
    } else {
      self.selected_wires.push(wire);
    }
    true
  }
  
  /// Add wire to selection (for Shift+click)
  pub fn add_wire_to_selection(&mut self, wire: Wire) -> bool {
    if !self.nodes.contains_key(&wire.source_node_id) 
       || !self.nodes.contains_key(&wire.destination_node_id) {
      return false;
    }
    self.selected_node_ids.clear();
    self.active_node_id = None;
    
    // Only add if not already selected
    if !self.selected_wires.iter().any(|w| w.eq(&wire)) {
      self.selected_wires.push(wire);
    }
    true
  }
  
  /// Check if a wire is selected
  pub fn is_wire_selected(&self, wire: &Wire) -> bool {
    self.selected_wires.iter().any(|w| w.eq(wire))
  }
  
  /// Get all selected wires
  pub fn get_selected_wires(&self) -> &Vec<Wire> {
    &self.selected_wires
  }
  
  // ===== COMMON SELECTION =====
  
  /// Clear all selection (nodes and wires)
  pub fn clear_selection(&mut self) {
    self.selected_node_ids.clear();
    self.active_node_id = None;
    self.selected_wires.clear();
  }
  
  /// Move all selected nodes by delta
  pub fn move_selected_nodes(&mut self, delta: DVec2) {
    for &node_id in &self.selected_node_ids {
      if let Some(node) = self.nodes.get_mut(&node_id) {
        node.position += delta;
      }
    }
  }
}
```

#### 1.3 Update `provide_gadget()`

Gadget should use `active_node_id` instead of `selected_node_id`:

```rust
pub fn provide_gadget(&self, structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
  if let Some(node_id) = self.active_node_id {
    let node = self.nodes.get(&node_id)?;
    return node.data.provide_gadget(structure_designer);
  }
  None
}
```

#### 1.4 Add `PartialEq` for `Wire`

```rust
impl PartialEq for Wire {
  fn eq(&self, other: &Self) -> bool {
    self.source_node_id == other.source_node_id
      && self.source_output_pin_index == other.source_output_pin_index
      && self.destination_node_id == other.destination_node_id
      && self.destination_argument_index == other.destination_argument_index
  }
}

impl Eq for Wire {}
```

#### 1.5 Update `delete_selected()`

Delete all selected nodes and wires:

```rust
pub fn delete_selected(&mut self) {
  // Handle selected nodes (multiple)
  if !self.selected_node_ids.is_empty() {
    let nodes_to_delete: Vec<u64> = self.selected_node_ids.iter().copied().collect();
    
    for node_id in nodes_to_delete {
      // Remove references from other nodes' arguments
      for other_node in self.nodes.values_mut() {
        for argument in other_node.arguments.iter_mut() {
          argument.argument_output_pins.remove(&node_id);
        }
      }
      
      // Clear return node if deleted
      if self.return_node_id == Some(node_id) {
        self.return_node_id = None;
      }
      
      // Remove from displayed nodes
      self.displayed_node_ids.remove(&node_id);
      
      // Remove the node
      self.nodes.remove(&node_id);
    }
    
    self.selected_node_ids.clear();
    self.active_node_id = None;
  }
  // Handle selected wires (multiple)
  else if !self.selected_wires.is_empty() {
    let wires_to_delete = std::mem::take(&mut self.selected_wires);
    
    for wire in wires_to_delete {
      if let Some(dest_node) = self.nodes.get_mut(&wire.destination_node_id) {
        if let Some(argument) = dest_node.arguments.get_mut(wire.destination_argument_index) {
          argument.argument_output_pins.remove(&wire.source_node_id);
        }
      }
    }
  }
}
```

---

### Phase 2: Update API Layer

#### 2.1 Update API types

**File:** `rust/src/api/structure_designer/structure_designer_api_types.rs`

```rust
pub struct APINodeView {
  // Change:
  // pub selected: bool,
  
  // To:
  pub selected: bool,    // True if in selection set
  pub active: bool,      // True if this is the active node
}
```

#### 2.2 Add new API functions

**File:** `rust/src/api/structure_designer/structure_designer_api.rs`

```rust
// Node selection
#[flutter_rust_bridge::frb(sync)]
pub fn toggle_node_selection(node_id: u64) -> bool { ... }

#[flutter_rust_bridge::frb(sync)]
pub fn add_node_to_selection(node_id: u64) -> bool { ... }

#[flutter_rust_bridge::frb(sync)]
pub fn select_nodes(node_ids: Vec<u64>) -> bool { ... }

#[flutter_rust_bridge::frb(sync)]
pub fn toggle_nodes_selection(node_ids: Vec<u64>) { ... }

#[flutter_rust_bridge::frb(sync)]
pub fn get_selected_node_ids() -> Vec<u64> { ... }

#[flutter_rust_bridge::frb(sync)]
pub fn move_selected_nodes(delta_x: f64, delta_y: f64) { ... }

#[flutter_rust_bridge::frb(sync)]
pub fn is_node_in_selection(node_id: u64) -> bool { ... }

// Wire selection
#[flutter_rust_bridge::frb(sync)]
pub fn toggle_wire_selection(
  source_node_id: u64, source_output_pin_index: i32,
  destination_node_id: u64, destination_argument_index: usize
) -> bool { ... }

#[flutter_rust_bridge::frb(sync)]
pub fn add_wire_to_selection(
  source_node_id: u64, source_output_pin_index: i32,
  destination_node_id: u64, destination_argument_index: usize
) -> bool { ... }

#[flutter_rust_bridge::frb(sync)]
pub fn get_selected_wires() -> Vec<APIWire> { ... }
```

#### 2.3 Update `node_network_view_from_node_network()`

```rust
APINodeView {
  // ...
  selected: node_network.is_node_selected(node.id),
  active: node_network.is_node_active(node.id),
  // ...
}

// For wires, add selected field to wire view or use a separate selected_wires list
```

#### 2.4 Add `APIWire` type (if not exists)

```rust
pub struct APIWire {
  pub source_node_id: u64,
  pub source_output_pin_index: i32,
  pub destination_node_id: u64,
  pub destination_argument_index: u64,
  pub selected: bool,
}
```

---

### Phase 3: Update Flutter Frontend

#### 3.1 Update `StructureDesignerModel`

**File:** `lib/structure_designer/structure_designer_model.dart`

```dart
// Add new methods:
void toggleNodeSelection(BigInt nodeId) {
  structure_designer_api.toggleNodeSelection(nodeId: nodeId);
  refreshFromKernel();
}

void addNodeToSelection(BigInt nodeId) {
  structure_designer_api.addNodeToSelection(nodeId: nodeId);
  refreshFromKernel();
}

void selectNodes(List<BigInt> nodeIds) {
  structure_designer_api.selectNodes(nodeIds: nodeIds);
  refreshFromKernel();
}

void toggleNodesSelection(List<BigInt> nodeIds) {
  structure_designer_api.toggleNodesSelection(nodeIds: nodeIds);
  refreshFromKernel();
}

Set<BigInt> getSelectedNodeIds() {
  return structure_designer_api.getSelectedNodeIds().toSet();
}

BigInt? getActiveNodeId() {
  if (nodeNetworkView == null) return null;
  for (final node in nodeNetworkView!.nodes.values) {
    if (node.active) return node.id;
  }
  return null;
}

// Wire selection methods:
void setSelectedWire(BigInt sourceNodeId, int sourcePinIndex, 
                     BigInt destNodeId, int destParamIndex) {
  structure_designer_api.selectWire(
    sourceNodeId: sourceNodeId,
    sourceOutputPinIndex: sourcePinIndex,
    destinationNodeId: destNodeId,
    destinationArgumentIndex: destParamIndex,
  );
  refreshFromKernel();
}

void toggleWireSelection(BigInt sourceNodeId, int sourcePinIndex,
                         BigInt destNodeId, int destParamIndex) {
  structure_designer_api.toggleWireSelection(
    sourceNodeId: sourceNodeId,
    sourceOutputPinIndex: sourcePinIndex,
    destinationNodeId: destNodeId,
    destinationArgumentIndex: destParamIndex,
  );
  refreshFromKernel();
}

void addWireToSelection(BigInt sourceNodeId, int sourcePinIndex,
                        BigInt destNodeId, int destParamIndex) {
  structure_designer_api.addWireToSelection(
    sourceNodeId: sourceNodeId,
    sourceOutputPinIndex: sourcePinIndex,
    destinationNodeId: destNodeId,
    destinationArgumentIndex: destParamIndex,
  );
  refreshFromKernel();
}

// Update drag methods for multi-selection:
void dragSelectedNodesPosition(Offset delta) {
  if (nodeNetworkView == null) return;
  for (final node in nodeNetworkView!.nodes.values) {
    if (node.selected) {
      node.position = APIVec2(
        x: node.position.x + delta.dx,
        y: node.position.y + delta.dy,
      );
    }
  }
  notifyListeners();
}

void updateSelectedNodesPositions() {
  structure_designer_api.moveSelectedNodes(
    deltaX: /* accumulated delta */,
    deltaY: /* accumulated delta */,
  );
  refreshFromKernel();
}
```

#### 3.2 Update `NodeWidget`

**File:** `lib/structure_designer/node_network/node_widget.dart`

```dart
void _handleNodeTap(BuildContext context, TapDownDetails details) {
  final model = Provider.of<StructureDesignerModel>(context, listen: false);
  
  if (HardwareKeyboard.instance.isControlPressed) {
    // Ctrl+click: toggle in selection
    model.toggleNodeSelection(node.id);
  } else if (HardwareKeyboard.instance.isShiftPressed) {
    // Shift+click: add to selection
    model.addNodeToSelection(node.id);
  } else {
    // Regular click: select only this node
    model.setSelectedNode(node.id);
  }
}

void _handleNodeDrag(BuildContext context, DragUpdateDetails details) {
  final model = Provider.of<StructureDesignerModel>(context, listen: false);
  final scale = getZoomScale(zoomLevel);
  final logicalDelta = details.delta / scale;
  
  if (node.selected) {
    // Drag all selected nodes together
    model.dragSelectedNodesPosition(logicalDelta);
  } else {
    // Clicking non-selected node: select it and drag alone
    model.setSelectedNode(node.id);
    model.dragNodePosition(node.id, logicalDelta);
  }
}

void _handleNodeDragEnd(BuildContext context) {
  final model = Provider.of<StructureDesignerModel>(context, listen: false);
  if (node.selected) {
    model.updateSelectedNodesPositions();
  } else {
    model.updateNodePosition(node.id);
  }
}

// Update decoration to show active vs selected:
BoxDecoration _getNodeDecoration() {
  Color borderColor;
  double borderWidth;
  
  if (node.error != null) {
    borderColor = NODE_BORDER_COLOR_ERROR;
    borderWidth = NODE_BORDER_WIDTH_NORMAL;
  } else if (node.active) {
    borderColor = NODE_BORDER_COLOR_ACTIVE;  // New: e.g., bright orange
    borderWidth = NODE_BORDER_WIDTH_SELECTED;
  } else if (node.selected) {
    borderColor = NODE_BORDER_COLOR_SELECTED; // e.g., dimmer orange
    borderWidth = NODE_BORDER_WIDTH_NORMAL;
  } else {
    borderColor = NODE_BORDER_COLOR_NORMAL;
    borderWidth = NODE_BORDER_WIDTH_NORMAL;
  }
  // ...
}
```

#### 3.3 Update wire tap handling in `NodeNetwork`

**File:** `lib/structure_designer/node_network/node_network.dart`

```dart
/// Handles tap on wires for selection, with modifier key support
void _handleWireTap(TapUpDetails details) {
  final painter = NodeNetworkPainter(model, panOffset: panOffset, zoomLevel: zoomLevel);
  final hit = painter.findWireAtPosition(details.localPosition);
  
  if (hit != null) {
    if (HardwareKeyboard.instance.isControlPressed) {
      // Ctrl+click: toggle wire in selection
      model.toggleWireSelection(
        hit.sourceNodeId,
        hit.sourcePinIndex,
        hit.destNodeId,
        hit.destParamIndex,
      );
    } else if (HardwareKeyboard.instance.isShiftPressed) {
      // Shift+click: add wire to selection
      model.addWireToSelection(
        hit.sourceNodeId,
        hit.sourcePinIndex,
        hit.destNodeId,
        hit.destParamIndex,
      );
    } else {
      // Regular click: select only this wire
      model.setSelectedWire(
        hit.sourceNodeId,
        hit.sourcePinIndex,
        hit.destNodeId,
        hit.destParamIndex,
      );
    }
  } else {
    // Clicked on empty space - clear selection
    model.clearSelection();
  }
}
```

#### 3.4 Update `NodeNetworkPainter` for multiple wire selection

**File:** `lib/structure_designer/node_network/node_network_painter.dart`

The painter already receives wire data; update to check `wire.selected` for each wire when drawing:

```dart
// When drawing each wire, check its selected state
final isSelected = wire.selected; // Now from API
if (isSelected) {
  // Draw with selected style (glow + thicker)
}
```

---

### Phase 4: Rectangle Selection (Nodes Only)

#### 4.1 Add rectangle selection state

**File:** `lib/structure_designer/node_network/node_network.dart`

```dart
class NodeNetworkState extends State<NodeNetwork> {
  // Add:
  Rect? _selectionRect;          // Current rectangle being drawn
  Offset? _selectionRectStart;   // Start point of rectangle drag
  
  void _handleSelectionRectStart(Offset position) {
    setState(() {
      _selectionRectStart = position;
      _selectionRect = Rect.fromPoints(position, position);
    });
  }
  
  void _handleSelectionRectUpdate(Offset position) {
    if (_selectionRectStart != null) {
      setState(() {
        _selectionRect = Rect.fromPoints(_selectionRectStart!, position);
      });
    }
  }
  
  void _handleSelectionRectEnd(StructureDesignerModel model) {
    if (_selectionRect != null && model.nodeNetworkView != null) {
      final scale = getZoomScale(_zoomLevel);
      
      // Find all nodes within the rectangle
      List<BigInt> nodesInRect = [];
      for (final entry in model.nodeNetworkView!.nodes.entries) {
        final node = entry.value;
        final nodeScreenPos = logicalToScreen(
          Offset(node.position.x, node.position.y),
          _panOffset,
          scale,
        );
        final nodeSize = getNodeSize(node, _zoomLevel);
        final nodeRect = Rect.fromLTWH(
          nodeScreenPos.dx,
          nodeScreenPos.dy,
          nodeSize.width,
          nodeSize.height,
        );
        
        if (_selectionRect!.overlaps(nodeRect)) {
          nodesInRect.add(node.id);
        }
      }
      
      // Apply selection based on modifier keys
      if (HardwareKeyboard.instance.isControlPressed) {
        model.toggleNodesSelection(nodesInRect);
      } else {
        model.selectNodes(nodesInRect);
      }
    }
    
    setState(() {
      _selectionRect = null;
      _selectionRectStart = null;
    });
  }
}
```

#### 4.2 Add rectangle selection gesture handling

Modify `_handlePointerDown` / `_handlePointerMove` / `_handlePointerUp` to handle left-click drag on empty space:

```dart
void _handleLeftMouseDown(PointerDownEvent event) {
  // Check if clicking on empty space (not on a node)
  if (!_isClickOnNode(model, event.localPosition)) {
    _handleSelectionRectStart(event.localPosition);
  }
}

void _handlePointerMove(PointerMoveEvent event) {
  // ... existing panning code ...
  
  // Rectangle selection
  if (_selectionRectStart != null && event.buttons == kPrimaryButton) {
    _handleSelectionRectUpdate(event.localPosition);
  }
}

void _handlePointerUp(PointerUpEvent event) {
  // ... existing panning code ...
  
  // Rectangle selection
  if (_selectionRectStart != null) {
    _handleSelectionRectEnd(widget.graphModel);
  }
}
```

#### 4.3 Draw selection rectangle

Add a `CustomPainter` or overlay widget to draw the selection rectangle:

```dart
Widget _buildSelectionRectangle() {
  if (_selectionRect == null) return const SizedBox.shrink();
  
  return Positioned.fromRect(
    rect: _selectionRect!,
    child: IgnorePointer(
      child: Container(
        decoration: BoxDecoration(
          border: Border.all(color: Colors.blue, width: 1),
          color: Colors.blue.withOpacity(0.1),
        ),
      ),
    ),
  );
}
```

---

### Phase 5: Update Dependent Code

#### 5.1 Update references to `selected_node_id`

Search for all usages of `selected_node_id` and update to use `active_node_id` where appropriate:

| Location | Change |
|----------|--------|
| `structure_designer.rs:get_selected_node_id_with_type()` | Use `active_node_id` |
| `structure_designer.rs:get_selected_node_eval_cache()` | Use `active_node_id` |
| `structure_designer.rs:is_node_type_active()` | Use `active_node_id` |
| `node_display_policy_resolver.rs` | Use `active_node_id` for visibility |
| `network_evaluator.rs` | Review - may need both selected set and active |
| Node properties panel (Dart) | Use `active_node_id` |

#### 5.2 Update serialization

**File:** `rust/src/structure_designer/cnnd_io/` (if selection is serialized)

Ensure `selected_node_ids` and `active_node_id` are properly serialized/deserialized if needed.

---

### Phase 6: Testing

#### 6.1 Unit tests

**File:** `rust/tests/structure_designer/node_network_test.rs`

Add tests for:

**Node selection:**
- `select_node()` - single selection clears previous (nodes and wires)
- `toggle_node_selection()` - adds/removes correctly, clears wires
- `add_node_to_selection()` - adds without clearing other nodes, clears wires
- `select_nodes()` - multi-select
- `toggle_nodes_selection()` - batch toggle
- `delete_selected()` - deletes all selected nodes
- `move_selected_nodes()` - moves all selected
- Active node tracking through selection changes

**Wire selection:**
- `select_wire()` - single selection clears previous (nodes and wires)
- `toggle_wire_selection()` - adds/removes correctly, clears nodes
- `add_wire_to_selection()` - adds without clearing other wires, clears nodes
- `delete_selected()` - deletes all selected wires
- `is_wire_selected()` - correctly identifies selected wires

#### 6.2 Integration tests

**Nodes:**
- Rectangle selection captures correct nodes (not wires)
- Modifier keys work correctly for nodes (Ctrl, Shift)
- Dragging selected node moves all selected
- Dragging non-selected node selects and drags only that node
- Properties panel shows active node
- Gadget appears for active node only
- Delete key removes all selected nodes

**Wires:**
- Modifier keys work correctly for wires (Ctrl, Shift)
- Clicking wire clears node selection
- Clicking node clears wire selection
- Delete key removes all selected wires
- Multiple wires can be selected and deleted together

---

## Summary of Files to Modify

### Rust Backend
1. `rust/src/structure_designer/node_network.rs` - Core selection state (multi-node + multi-wire)
2. `rust/src/api/structure_designer/structure_designer_api.rs` - API functions for node/wire selection
3. `rust/src/api/structure_designer/structure_designer_api_types.rs` - Add `active` field to nodes, `selected` to wires
4. `rust/src/structure_designer/structure_designer.rs` - Update helper methods
5. `rust/src/structure_designer/node_display_policy_resolver.rs` - Use active_node_id
6. `rust/src/structure_designer/evaluator/network_evaluator.rs` - Review selection usage

### Flutter Frontend
1. `lib/structure_designer/structure_designer_model.dart` - Model methods for node/wire selection
2. `lib/structure_designer/node_network/node_widget.dart` - Click/drag handlers, styling (active vs selected)
3. `lib/structure_designer/node_network/node_network.dart` - Rectangle selection, wire tap with modifiers
4. `lib/structure_designer/node_network/node_network_painter.dart` - Multi-wire selection rendering

### Generated (after FRB codegen)
1. `lib/src/rust/api/structure_designer/structure_designer_api.dart`
2. `lib/src/rust/api/structure_designer/structure_designer_api_types.dart`

---

## Visual Design

### Selection States

| State | Border Color | Border Width | Glow |
|-------|--------------|--------------|------|
| Normal | Blue accent | 2px | None |
| Selected (not active) | Orange (dimmer) | 2px | Subtle |
| Active (selected) | Bright orange | 3px | Full glow |
| Error | Red | 2px | Red glow |

### Selection Rectangle

- Border: 1px solid blue
- Fill: Blue with 10% opacity
- Drawn on top of wires but below nodes
