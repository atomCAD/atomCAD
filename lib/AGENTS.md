# Flutter Frontend - Agent Instructions

## Overview

The Flutter frontend provides the cross-platform UI for atomCAD. It communicates with the Rust backend via Flutter Rust Bridge (FRB) bindings.

## Directory Structure

```
lib/
├── main.dart              # Entry point (GUI + CLI modes)
├── common/                # Shared UI widgets and utilities
├── inputs/                # Input handling
├── structure_designer/    # Main Structure Designer UI (see structure_designer/AGENTS.md)
│   ├── node_network/      # Node network editor
│   ├── node_data/         # Node property editors (incl. record_construct/destructure/product)
│   ├── node_networks_list/# Unified user-types panel (networks + record defs)
│   └── schema_editor.dart # Record-def field editor (swaps in for the network editor)
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

### Showing an Error Message (never hand-roll one)

Every user-facing failure message must be **extractable** — issue #359. The rule,
and the widgets that implement it, live in `lib/common/error_display.dart`:

> **Transient surfaces get a copy _action_; persistent surfaces get real
> _selection_ (plus a copy button).**

Flutter's `Tooltip` and `SnackBar` are overlay entries that vanish on
pointer-exit / on a timer, and a tooltip does not accept hit-testing — there is
no drag-select to be had on either, whatever widget goes inside. So they get a
**Copy** affordance instead of being "made selectable"; only surfaces that hold
still are selectable.

| Surface | Use | Never |
|---|---|---|
| Inline red box in a panel/dialog | `ErrorBanner(message: …)` | a hand-rolled `Container` + `Text` |
| Bottom toast for a **failure** | `showErrorSnackBar(context, msg)` (or `showErrorSnackBarOn(messenger, msg)` after an async gap) | `SnackBar(backgroundColor: Colors.red…)` |
| Bottom toast that merely *carries* error text | `showCopyableSnackBar(context, msg)` | — |
| Modal failure dialog | `showErrorDialog(context:, title:, message:)` | `showDraggableAlertDialog(content: Text(err))` |
| A tooltip-only error surface | keep the tooltip, add a copy action elsewhere (context menu, adjacent button) | trying to make the tooltip selectable |

Purely *informational* snackbars ("Saved foo.cnnd", "Activated: X", "Atom is
fully bonded") deliberately keep their plain styling and short duration — there
is nothing there worth reporting, and a Copy action on them is noise.

`lib/structure_designer/error_report.dart` renders the design's unified error
list (`doc/design_error_management.md` D1) as a plain-text report for
*Edit > Copy all problems* and the error picker's *Copy all*. It is also the
**shared home of the root-cause grouping** (`groupErrorsByRootCause`,
`ErrorGroup`, `isNavigableError`) that the panel badge and picker consume, so
the copied report and the on-screen list can never disagree about what counts as
one problem. It is pure Dart over the generated API data classes — unit-tested
in `test/error_report_test.dart`, which is the only thing in `test/` and runs in
well under a second (unlike `integration_test/`, which agents must not run).

### Dialogs Must Be Draggable

All dialogs in this application **must be draggable**. Use the `DraggableDialog` widget from `lib/common/draggable_dialog.dart`.

- For simple title + content + actions dialogs, use the `showDraggableAlertDialog()` helper — it is a drop-in replacement for `showDialog()` + `AlertDialog`.
- For custom dialog layouts, use `DraggableDialog` directly and manage padding/layout inside its `child`.
- Always set `barrierDismissible: false` on the outer `showDialog` call — `DraggableDialog` handles its own dismissal barrier.
- **Never** use a plain `AlertDialog` or non-draggable `showDialog` for user-facing dialogs.

```dart
import 'package:flutter_cad/common/draggable_dialog.dart';

// Simple case — drop-in replacement for AlertDialog:
showDraggableAlertDialog(
  context: context,
  title: const Text('My Title'),
  content: myContentWidget,
  actions: [
    TextButton(onPressed: () => Navigator.of(context).pop(), child: const Text('Cancel')),
    ElevatedButton(onPressed: onApply, child: const Text('Apply')),
  ],
);

// Custom layout case:
showDialog(
  context: context,
  barrierDismissible: false,
  builder: (context) => DraggableDialog(
    width: 400,
    dismissible: true,
    child: Padding(
      padding: const EdgeInsets.all(24),
      child: Column(mainAxisSize: MainAxisSize.min, children: [ /* ... */ ]),
    ),
  ),
);
```

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
