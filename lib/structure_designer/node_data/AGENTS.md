# Node Data Editors - Agent Instructions

Per-node-type property editor widgets. Each node type has a corresponding editor widget displayed in the properties panel when that node is selected.

## Files

- `node_data_widget.dart` - Router: selects correct editor widget by node type name
- `node_editor_header.dart` - Shared header (node name, type info)
- `node_description_button.dart` - Shows node description tooltip
- `network_description_editor.dart` - Editor for network-level description/summary
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
- `atom_fill_editor.dart` → AtomFill node
- `vec3_editor.dart` → Vec3 node

Exception: `atom_trans.dart` (no `_editor` suffix, legacy).

## Node Types Without Custom Editors

Some nodes (like boolean operations: Union, Intersect, Diff) have no editable properties — they only receive wired inputs. These don't need dedicated editor files; `node_data_widget.dart` shows the default header only.
