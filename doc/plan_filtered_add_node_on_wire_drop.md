# Implementation Plan: Filtered Add Node on Wire Drop

## Overview

When a user drags a wire from a pin and releases it in empty space (not on a valid target pin), open the Add Node dialog filtered to show only nodes with compatible pins. After selection, auto-connect the new node.

## Scope

- **In scope:** Dragging from output pins, dragging from input pins (symmetric implementation)
- **Approach:** Rust-side filtering via new API

---

## Phase 1: Rust API Extension

### 1.1 Add new API function for filtered node lookup

**File:** `rust/src/api/structure_designer/structure_designer_api.rs`

No changes needed to `APINodeTypeView` — the existing structure (name, description, category) is sufficient since filtering happens in Rust.

```rust
/// Returns node types that have at least one pin compatible with the given type.
/// 
/// - `source_type_str`: The data type being dragged (serialized string)
/// - `dragging_from_output`: true if dragging from output pin, false if from input pin
/// 
/// When dragging from OUTPUT: find nodes with compatible INPUT pins
/// When dragging from INPUT: find nodes with compatible OUTPUT pins
pub fn get_compatible_node_types(
    source_type_str: String,
    dragging_from_output: bool,
) -> Option<Vec<APINodeCategoryView>> {
    // Parse source_type_str to DataType
    // Filter nodes based on compatibility
    // Return filtered categories
}
```

### 1.4 Implement filtering logic

**File:** `rust/src/structure_designer/node_type_registry.rs`

Add method:
```rust
pub fn get_compatible_node_types(
    &self,
    source_type: &DataType,
    dragging_from_output: bool,
) -> Vec<APINodeCategoryView> {
    // For each node type:
    //   If dragging_from_output:
    //     Check if ANY input pin accepts source_type via can_be_converted_to
    //   Else:
    //     Check if output pin can be converted to source_type
    // Return matching nodes grouped by category
}
```

---

## Phase 2: Flutter Model Layer

### 2.1 Add state for pending wire connection

**File:** `lib/structure_designer/structure_designer_model.dart`

The existing `DraggedWire` class already has `startPin` which contains the data type. No changes needed to model state.

### 2.2 Add method to fetch compatible nodes

**File:** `lib/structure_designer/structure_designer_model.dart`

```dart
Future<List<APINodeCategoryView>?> getCompatibleNodeTypes(
    String sourceType, bool draggingFromOutput) async {
  return api.getCompatibleNodeTypes(
      sourceTypeStr: sourceType, draggingFromOutput: draggingFromOutput);
}
```

---

## Phase 3: Update Add Node Popup

### 3.1 Add optional filter parameters to popup

**File:** `lib/structure_designer/node_network/add_node_popup.dart`

```dart
class AddNodePopup extends StatefulWidget {
  final String? filterByCompatibleType;  // NEW
  final bool? draggingFromOutput;         // NEW
  
  const AddNodePopup({
    super.key,
    this.filterByCompatibleType,
    this.draggingFromOutput,
  });
  // ...
}
```

### 3.2 Modify `initState` to use filtered data when provided

```dart
@override
void initState() {
  super.initState();
  if (widget.filterByCompatibleType != null) {
    // Call new API for filtered nodes
    final categories = getCompatibleNodeTypes(
      sourceTypeStr: widget.filterByCompatibleType!,
      draggingFromOutput: widget.draggingFromOutput ?? true,
    );
    // ...
  } else {
    // Existing behavior
    final categories = getNodeTypeViews();
    // ...
  }
}
```

### 3.3 Update `showAddNodePopup` helper

```dart
Future<String?> showAddNodePopup(
  BuildContext context, {
  String? filterByCompatibleType,
  bool? draggingFromOutput,
}) {
  return showDialog<String>(
    context: context,
    barrierDismissible: true,
    builder: (context) => AddNodePopup(
      filterByCompatibleType: filterByCompatibleType,
      draggingFromOutput: draggingFromOutput,
    ),
  );
}
```

---

## Phase 4: Wire Drop Handling

### 4.1 Modify `onDragEnd` in PinWidget

**File:** `lib/structure_designer/node_network/node_widget.dart`

Current:
```dart
onDragEnd: (details) {
  Provider.of<StructureDesignerModel>(context, listen: false)
      .cancelDragWire();
}
```

New approach — instead of canceling immediately, trigger a callback that the NodeNetwork widget can handle:

```dart
onDragEnd: (details) {
  final model = Provider.of<StructureDesignerModel>(context, listen: false);
  // Store the drag info before clearing
  final dragInfo = model.draggedWire;
  if (dragInfo != null) {
    // Notify parent to handle the drop (will show popup if in empty space)
    model.handleWireDropInEmptySpace(
      dragInfo.startPin,
      dragInfo.wireEndPosition,
    );
  }
  model.cancelDragWire();
}
```

### 4.2 Add wire drop handler in model

**File:** `lib/structure_designer/structure_designer_model.dart`

```dart
// Callback that NodeNetwork widget sets
void Function(PinReference startPin, Offset dropPosition)? onWireDroppedInEmptySpace;

void handleWireDropInEmptySpace(PinReference startPin, Offset dropPosition) {
  onWireDroppedInEmptySpace?.call(startPin, dropPosition);
}
```

### 4.3 Handle the callback in NodeNetwork

**File:** `lib/structure_designer/node_network/node_network.dart`

In `initState` or appropriate lifecycle:
```dart
widget.graphModel.onWireDroppedInEmptySpace = (startPin, dropPosition) async {
  final isOutput = startPin.isOutput;
  final dataType = startPin.dataType;
  
  final selectedNode = await showAddNodePopup(
    context,
    filterByCompatibleType: dataType,
    draggingFromOutput: isOutput,
  );
  
  if (selectedNode != null) {
    // Create node at drop position
    final newNodeId = widget.graphModel.createNode(selectedNode, dropPosition);
    
    // Auto-connect: find compatible pin on new node and connect
    widget.graphModel.autoConnectWire(startPin, newNodeId);
  }
};
```

---

## Phase 5: Auto-Connect After Node Creation

### 5.1 Add auto-connect API

**File:** `rust/src/api/structure_designer/structure_designer_api.rs`

```rust
/// Finds the first compatible pin on the target node and connects it.
/// Returns true if a connection was made.
pub fn auto_connect_to_node(
    source_node_id: i64,
    source_pin_index: i32,
    source_is_output: bool,
    target_node_id: i64,
) -> bool {
    // Find compatible pin on target node
    // Call existing connect logic
}
```

### 5.2 Expose in Flutter model

**File:** `lib/structure_designer/structure_designer_model.dart`

```dart
void autoConnectWire(PinReference sourcePin, int targetNodeId) {
  api.autoConnectToNode(
    sourceNodeId: sourcePin.nodeId,
    sourcePinIndex: sourcePin.pinIndex,
    sourceIsOutput: sourcePin.isOutput,
    targetNodeId: targetNodeId,
  );
  refreshFromKernel();
}
```

---

## Phase 6: Regenerate FFI Bindings

After Rust API changes:
```bash
flutter_rust_bridge_codegen generate
```

---

## Testing Plan

1. **Unit test (Rust):** `get_compatible_node_types` returns correct nodes for various data types
2. **Unit test (Rust):** `auto_connect_to_node` connects to first compatible pin
3. **Integration test:** Drag from Geometry output → release in empty space → verify only nodes with Geometry-compatible inputs shown
4. **Integration test:** Select node from filtered list → verify auto-connection created
5. **Edge cases:**
   - Drag from function pin (should filter by function type)
   - No compatible nodes exist (show empty state or message)
   - Array pins (T should match [T] inputs)

---

## Implementation Order

| Step | Task | Estimated Effort |
|------|------|------------------|
| 1 | Implement `get_compatible_node_types` in Rust | Medium |
| 2 | Implement `auto_connect_to_node` in Rust | Small |
| 3 | Regenerate FFI bindings | Trivial |
| 4 | Update `AddNodePopup` to accept filter params | Small |
| 5 | Modify `PinWidget.onDragEnd` and wire drop flow | Medium |
| 6 | Wire up auto-connect after node creation | Small |
| 7 | Testing and polish | Medium |

**Total estimated effort:** ~1-2 days

---

## Notes

- Input pin dragging works symmetrically — same code path, just `draggingFromOutput: false`
- The popup position could optionally appear near the drop location (nice-to-have)
- Consider showing a visual hint during drag when over empty space (e.g., "+" icon) to indicate the feature exists
