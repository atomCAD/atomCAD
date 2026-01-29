# CLI Display Settings Implementation Plan

## Overview

Add support for controlling display preferences (atomic visualization, geometry visualization, node display policy, background settings) from `atomcad-cli`.

## Current State

- **Display preferences** are fully implemented in Rust backend (`rust/src/api/structure_designer/structure_designer_preferences.rs`)
- **API functions exist**: `get_structure_designer_preferences()` and `set_structure_designer_preferences()`
- **No HTTP endpoint** exists for display settings
- **No CLI command** exists for display settings

## Proposed CLI Interface

```bash
# Get all display settings (returns JSON)
atomcad-cli display

# Atomic visualization
atomcad-cli display --atomic-viz ball-and-stick
atomcad-cli display --atomic-viz space-filling

# Geometry visualization
atomcad-cli display --geometry-viz surface-splatting
atomcad-cli display --geometry-viz solid
atomcad-cli display --geometry-viz wireframe

# Node display policy
atomcad-cli display --node-policy manual
atomcad-cli display --node-policy prefer-selected
atomcad-cli display --node-policy prefer-frontier

# Background color
atomcad-cli display --background 30,30,30

# Combine multiple settings
atomcad-cli display --atomic-viz space-filling --geometry-viz wireframe
```

---

## Phase 1: HTTP Endpoint Implementation

**Goal:** Add `/display` endpoint to the HTTP server.

### Files to Modify

1. **`lib/ai_assistant/http_server.dart`**

### Tasks

#### 1.1 Add import for preferences API

```dart
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_preferences.dart'
    as prefs_api;
```

#### 1.2 Add `/display` route in `_handleRequest` switch

```dart
case '/display':
  await _handleDisplay(request);
  break;
```

#### 1.3 Add endpoint documentation in class docstring

```dart
/// - `GET /display` - Get current display preferences as JSON
/// - `GET /display?atomic-viz=...&geometry-viz=...` - Set display preferences
```

#### 1.4 Implement `_handleDisplay` method

The handler should:
- Accept GET requests only
- Parse query parameters for each setting type
- Call `sd_api.getStructureDesignerPreferences()` to get current state
- If parameters provided, modify and call `sd_api.setStructureDesignerPreferences()`
- Call `onNetworkEdited?.call()` to trigger UI refresh after changes
- Return current state as JSON

**Query parameters to support:**
| Parameter | Values |
|-----------|--------|
| `atomic-viz` | `ball-and-stick`, `space-filling` |
| `geometry-viz` | `surface-splatting`, `solid`, `wireframe` |
| `node-policy` | `manual`, `prefer-selected`, `prefer-frontier` |
| `background` | `R,G,B` (0-255 each) |

**Response format:**
```json
{
  "success": true,
  "display": {
    "atomic_visualization": "ball-and-stick",
    "geometry_visualization": "solid",
    "node_display_policy": "prefer-selected",
    "background_color": [30, 30, 30]
  }
}
```

### Implementation Notes

- Need to map string values to enum types from the preferences API
- The Rust enums are: `AtomicStructureVisualization`, `GeometryVisualization`, `NodeDisplayPolicy`
- Check the generated Dart bindings in `lib/src/rust/api/structure_designer/structure_designer_preferences.dart` for exact type names

---

## Phase 2: CLI Command Implementation

**Goal:** Add `display` command to the CLI.

### Files to Modify

1. **`bin/atomcad_cli.dart`**

### Tasks

#### 2.1 Add display command parser

```dart
final displayParser = ArgParser()
  ..addOption('atomic-viz',
      help: 'Atomic visualization mode (ball-and-stick, space-filling)')
  ..addOption('geometry-viz',
      help: 'Geometry visualization mode (surface-splatting, solid, wireframe)')
  ..addOption('node-policy',
      help: 'Node display policy (manual, prefer-selected, prefer-frontier)')
  ..addOption('background',
      help: 'Background color as R,G,B (0-255)');

parser.addCommand('display', displayParser);
```

#### 2.2 Add case in command switch

```dart
case 'display':
  await _runDisplay(serverUrl, command);
  break;
```

#### 2.3 Update `_printUsage()` with display command help

```dart
stdout.writeln('  atomcad-cli display                   Get current display settings');
stdout.writeln('  atomcad-cli display --atomic-viz <mode>');
stdout.writeln('                                        Set atomic visualization');
stdout.writeln('  atomcad-cli display --geometry-viz <mode>');
stdout.writeln('                                        Set geometry visualization');
stdout.writeln('  atomcad-cli display --node-policy <policy>');
stdout.writeln('                                        Set node display policy');
stdout.writeln('  atomcad-cli display --background R,G,B');
stdout.writeln('                                        Set background color');
```

#### 2.4 Implement `_runDisplay` function

```dart
Future<void> _runDisplay(String serverUrl, ArgResults args) async {
  try {
    final queryParams = <String, String>{};

    if (args['atomic-viz'] != null) {
      queryParams['atomic-viz'] = args['atomic-viz'];
    }
    if (args['geometry-viz'] != null) {
      queryParams['geometry-viz'] = args['geometry-viz'];
    }
    if (args['node-policy'] != null) {
      queryParams['node-policy'] = args['node-policy'];
    }
    if (args['background'] != null) {
      queryParams['background'] = args['background'];
    }

    final uri = Uri.parse('$serverUrl/display')
        .replace(queryParameters: queryParams.isEmpty ? null : queryParams);

    final response = await http.get(uri).timeout(const Duration(seconds: 10));

    if (response.statusCode == 200) {
      stdout.writeln(response.body);
    } else {
      stderr.writeln('Error: Server returned ${response.statusCode}');
      stderr.writeln(response.body);
      exit(1);
    }
  } catch (e) {
    stderr.writeln('Error: Failed to connect to atomCAD: $e');
    exit(1);
  }
}
```

#### 2.5 Add REPL support

Add `display` command handling in the REPL switch:

```dart
case 'display':
  await _runDisplayRepl(serverUrl, parts);
  break;
```

Update `_printReplHelp()` with display commands.

Implement `_runDisplayRepl` function for REPL-style argument parsing.

---

## Phase 3: Skill Documentation Update

**Goal:** Update the atomCAD skill documentation with the new `display` command.

### Files to Modify

1. **`.claude/skills/atomcad/skill.md`**

### Tasks

#### 3.1 Add Display Settings section after Camera Control

```markdown
### Display Settings

Control viewport display preferences including atomic visualization and geometry rendering.

```bash
# Get current display settings (returns JSON)
atomcad-cli display

# Set atomic visualization mode
atomcad-cli display --atomic-viz ball-and-stick    # Small atoms with bond sticks
atomcad-cli display --atomic-viz space-filling     # Large van der Waals spheres

# Set geometry visualization mode
atomcad-cli display --geometry-viz surface-splatting  # Implicit SDF rendering
atomcad-cli display --geometry-viz solid              # Solid mesh (default)
atomcad-cli display --geometry-viz wireframe          # Wireframe mesh

# Set node display policy
atomcad-cli display --node-policy manual           # User controls visibility
atomcad-cli display --node-policy prefer-selected  # Show selected node (default)
atomcad-cli display --node-policy prefer-frontier  # Show output nodes

# Set background color
atomcad-cli display --background 30,30,30          # Dark gray (default)
atomcad-cli display --background 255,255,255       # White

# Combine multiple settings
atomcad-cli display --atomic-viz space-filling --geometry-viz wireframe
```

**Response:** Returns JSON with current display state:
```json
{
  "success": true,
  "display": {
    "atomic_visualization": "ball-and-stick",
    "geometry_visualization": "solid",
    "node_display_policy": "prefer-selected",
    "background_color": [30, 30, 30]
  }
}
```
```

#### 3.2 Update REPL Commands section

Add display command to REPL section:

```markdown
- `display` — Get/set display preferences
```

#### 3.3 Update Typical AI Agent Workflow example

Add display settings to the workflow example to show how to switch visualization modes before screenshots.

---

## Phase 4: Testing & Validation

**Goal:** Verify the implementation works correctly.

### Manual Testing Checklist

1. [ ] Start atomCAD
2. [ ] Run `atomcad-cli display` - should return current settings as JSON
3. [ ] Run `atomcad-cli display --atomic-viz space-filling` - verify UI updates
4. [ ] Run `atomcad-cli display --atomic-viz ball-and-stick` - verify UI updates
5. [ ] Run `atomcad-cli display --geometry-viz wireframe` - verify UI updates
6. [ ] Run `atomcad-cli display --geometry-viz solid` - verify UI updates
7. [ ] Run `atomcad-cli display --node-policy manual` - verify policy changes
8. [ ] Run `atomcad-cli display --background 255,255,255` - verify background changes
9. [ ] Test combined flags: `atomcad-cli display --atomic-viz space-filling --background 0,0,0`
10. [ ] Test REPL mode: `display`, `display --atomic-viz space-filling`
11. [ ] Test invalid values return appropriate errors

---

## Summary

| Phase | Effort | Files Modified |
|-------|--------|----------------|
| Phase 1: HTTP Endpoint | Medium | `lib/ai_assistant/http_server.dart` |
| Phase 2: CLI Command | Medium | `bin/atomcad_cli.dart` |
| Phase 3: Documentation | Low | `.claude/skills/atomcad/skill.md` |
| Phase 4: Testing | Low | (manual testing) |

**No Rust changes required** - the backend API already exists.
