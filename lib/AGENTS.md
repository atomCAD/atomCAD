# Flutter Frontend - Agent Instructions

## Overview

The Flutter frontend provides the cross-platform UI for atomCAD. It communicates with the Rust backend via Flutter Rust Bridge (FRB) bindings.

## Directory Structure

```
lib/
├── main.dart              # Entry point (GUI + CLI modes)
├── common/                # Shared UI widgets and utilities
├── inputs/                # Input handling
├── structure_designer/    # Main Structure Designer UI
│   ├── node_network/      # Node network editor
│   ├── node_data/         # Node property editors
│   └── node_networks_list/# Network list panel
└── src/rust/              # Generated FRB bindings (DO NOT EDIT)
```

## Code Conventions

### State Management

- Use `ChangeNotifier` + `Provider` for state
- `StructureDesignerModel` is the main state container
- Access via `Provider.of<StructureDesignerModel>(context)` or `Consumer`

### API Imports

Always prefix Rust API imports to avoid conflicts:

```dart
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
import 'package:flutter_cad/src/rust/api/common_api.dart' as common_api;

// Usage
sd_api.someFunction();
common_api.setCameraTransform(transform: transform);
```

### Naming

- Widgets: `PascalCase` (e.g., `NodeNetworkWidget`)
- Files: `snake_case.dart` (e.g., `node_network_widget.dart`)
- Variables/functions: `camelCase`
- Constants: `SCREAMING_SNAKE_CASE` (e.g., `NODE_WIDTH`)

## Key Files

| File | Purpose |
|------|---------|
| `main.dart` | App entry point, CLI parsing, GUI initialization |
| `structure_designer/structure_designer.dart` | Main editor widget with menu bar |
| `structure_designer/structure_designer_model.dart` | Central state management |
| `structure_designer/node_network/node_network.dart` | Node graph editor widget |
| `common/cad_viewport.dart` | 3D viewport base class |

## Adding New Node Property Editors

1. Create `lib/structure_designer/node_data/my_node_editor.dart`
2. Register in `node_data_widget.dart`

## Common Patterns

### Calling Rust API

```dart
void addNode() {
  sd_api.addNode(nodeType: 'Sphere', x: 100.0, y: 200.0);
  model.refreshFromKernel(); // Update UI after Rust state change
}
```

### Vector Conversion

```dart
import 'package:flutter_cad/common/api_utils.dart';

final vec = apiVec3ToVector3(apiVec3);
final apiVec = vector3ToApiVec3(vec);
```

## Generated Code

`src/rust/` contains generated FRB bindings — **do not edit**.

Regenerate after Rust API changes:
```powershell
flutter_rust_bridge_codegen generate
```
