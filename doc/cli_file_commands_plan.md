# CLI File Commands Design and Implementation Plan

## Overview

This document describes the design and implementation plan for adding `.cnnd` file load/save commands to `atomcad-cli`. Currently, file operations must be done manually through the GUI; this feature enables AI agents and scripts to manage project files programmatically.

## Command Design

### 1. `load` - Load a .cnnd file

```bash
# Load a .cnnd file into the running atomCAD instance
atomcad-cli load <path>
atomcad-cli load design.cnnd

# Force load (discard unsaved changes without error)
atomcad-cli load <path> --force
```

**Behavior:**
- Without `--force`: Returns error if there are unsaved changes
- With `--force`: Discards unsaved changes and loads the file
- Returns error if file doesn't exist or is invalid
- On success, prints confirmation with file path and network count

**Output examples:**
```
# Success
Loaded: /path/to/design.cnnd (3 networks)

# Error - unsaved changes
Error: Unsaved changes exist. Use 'save <path>' to save first, or --force to discard.

# Error - file not found
Error: File not found: /path/to/missing.cnnd

# Error - invalid file
Error: Failed to load file: Invalid JSON at line 42
```

### 2. `save` - Save to a .cnnd file

```bash
# Save to a specific path (Save As)
atomcad-cli save <path>
atomcad-cli save my_design.cnnd

# Save to current file
atomcad-cli save
```

**Behavior:**
- `save` (no path): Saves to current file path; fails if no file is loaded
- `save <path>`: Saves to specified path (overwrites if exists)
- Updates the current file path after successful save
- Clears dirty flag on success

**Output examples:**
```
# Success
Saved: /path/to/design.cnnd

# Error - no file loaded
Error: No file loaded. Specify a path: atomcad-cli save <path>
```

### 3. `file` - Show current file status

```bash
# Show current file info
atomcad-cli file
```

**Output examples:**
```
# With file loaded, modified
File: /path/to/design.cnnd
Modified: yes
Networks: 3

# With file loaded, not modified
File: /path/to/design.cnnd
Modified: no
Networks: 3

# No file loaded, modified
File: (none)
Modified: yes
Networks: 2

# No file loaded, not modified
File: (none)
Modified: no
Networks: 1
```

### 4. `new` - Create new empty project

```bash
# Clear all networks and start fresh
atomcad-cli new

# Force (skip unsaved changes warning)
atomcad-cli new --force
```

**Behavior:**
- Without `--force`: Returns error if there are unsaved changes
- With `--force`: Discards unsaved changes
- Clears all networks and creates a fresh "Main" network
- Clears current file path
- Clears dirty flag

**Output examples:**
```
# Success
New project created.

# Error - unsaved changes
Error: Unsaved changes exist. Use 'save <path>' to save first, or --force to discard.
```

---

## Implementation Plan

### Architecture Overview

The CLI uses HTTP-based communication:

```
atomcad-cli (Dart)          atomCAD (Flutter + Rust)
     │                              │
     │  HTTP Request                │
     ├─────────────────────────────►│
     │                              │  http_server.dart
     │                              │       │
     │                              │       ▼
     │                              │  structure_designer_api.rs
     │                              │       │
     │                              │       ▼
     │  HTTP Response               │  structure_designer.rs
     │◄─────────────────────────────┤
     │                              │
```

### Existing Infrastructure

**Rust API (already exists in `rust/src/api/structure_designer/structure_designer_api.rs`):**
- `load_node_networks(file_path: String) -> APIResult` (line 2487)
- `save_node_networks_as(file_path: String) -> bool` (line 2430)
- `save_node_networks() -> bool` (line 2446)
- `is_design_dirty() -> bool` (line 2463)
- `get_design_file_path() -> Option<String>` (line 2475)

**Missing Rust API:**
- `new_project()` - Clear all networks and reset state
- `get_network_count() -> i32` - For status display

### Implementation Steps

#### Step 1: Add Missing Rust API Functions

**File:** `rust/src/api/structure_designer/structure_designer_api.rs`

```rust
/// Clear all networks and create a fresh project
#[flutter_rust_bridge::frb(sync)]
pub fn new_project() {
    let mut cad_instance = get_cad_instance();
    cad_instance.structure_designer.new_project();
}

/// Get the number of networks
#[flutter_rust_bridge::frb(sync)]
pub fn get_network_count() -> i32 {
    let cad_instance = get_cad_instance();
    cad_instance.structure_designer.node_type_registry.node_networks.len() as i32
}
```

**File:** `rust/src/structure_designer/structure_designer.rs`

```rust
/// Clear all networks and create a fresh project
pub fn new_project(&mut self) {
    self.node_type_registry.node_networks.clear();
    self.node_type_registry.add_node_network("Main".to_string());
    self.active_node_network_name = Some("Main".to_string());
    self.file_path = None;
    self.is_dirty = false;
}
```

#### Step 2: Add HTTP Endpoints

**File:** `lib/ai_assistant/http_server.dart`

Add new route handlers:

```dart
// GET /file - Get current file status
case '/file':
  return _handleFileStatus(request);

// POST /load - Load a .cnnd file
case '/load':
  return _handleLoad(request);

// POST /save - Save to a .cnnd file
case '/save':
  return _handleSave(request);

// POST /new - Create new project
case '/new':
  return _handleNew(request);
```

**Handler implementations:**

```dart
Future<Response> _handleFileStatus(Request request) async {
  final filePath = sd_api.getDesignFilePath();
  final isDirty = sd_api.isDesignDirty();
  final networkCount = sd_api.getNetworkCount();

  return Response.ok(jsonEncode({
    'success': true,
    'file_path': filePath,
    'modified': isDirty,
    'network_count': networkCount,
  }));
}

Future<Response> _handleLoad(Request request) async {
  final params = request.url.queryParameters;
  final filePath = params['path'];
  final force = params['force'] == 'true';

  if (filePath == null || filePath.isEmpty) {
    return Response.ok(jsonEncode({
      'success': false,
      'error': 'Missing required parameter: path',
    }));
  }

  // Check for unsaved changes
  if (!force && sd_api.isDesignDirty()) {
    return Response.ok(jsonEncode({
      'success': false,
      'error': "Unsaved changes exist. Use 'save <path>' to save first, or --force to discard.",
    }));
  }

  // Check if file exists
  final file = File(filePath);
  if (!await file.exists()) {
    return Response.ok(jsonEncode({
      'success': false,
      'error': 'File not found: $filePath',
    }));
  }

  // Load the file
  final result = sd_api.loadNodeNetworks(filePath: filePath);

  if (result.success) {
    final networkCount = sd_api.getNetworkCount();
    _notifyNetworkEdited();
    return Response.ok(jsonEncode({
      'success': true,
      'file_path': filePath,
      'network_count': networkCount,
    }));
  } else {
    return Response.ok(jsonEncode({
      'success': false,
      'error': 'Failed to load file: ${result.errorMessage}',
    }));
  }
}

Future<Response> _handleSave(Request request) async {
  final params = request.url.queryParameters;
  final filePath = params['path'];

  if (filePath == null || filePath.isEmpty) {
    // Save to current file
    final currentPath = sd_api.getDesignFilePath();
    if (currentPath == null) {
      return Response.ok(jsonEncode({
        'success': false,
        'error': 'No file loaded. Specify a path: atomcad-cli save <path>',
      }));
    }

    final success = sd_api.saveNodeNetworks();
    if (success) {
      return Response.ok(jsonEncode({
        'success': true,
        'file_path': currentPath,
      }));
    } else {
      return Response.ok(jsonEncode({
        'success': false,
        'error': 'Failed to save file.',
      }));
    }
  } else {
    // Save to specified path (overwrites if exists)
    final success = sd_api.saveNodeNetworksAs(filePath: filePath);
    if (success) {
      return Response.ok(jsonEncode({
        'success': true,
        'file_path': filePath,
      }));
    } else {
      return Response.ok(jsonEncode({
        'success': false,
        'error': 'Failed to save file.',
      }));
    }
  }
}

Future<Response> _handleNew(Request request) async {
  final params = request.url.queryParameters;
  final force = params['force'] == 'true';

  // Check for unsaved changes
  if (!force && sd_api.isDesignDirty()) {
    return Response.ok(jsonEncode({
      'success': false,
      'error': "Unsaved changes exist. Use 'save <path>' to save first, or --force to discard.",
    }));
  }

  sd_api.newProject();
  _notifyNetworkEdited();

  return Response.ok(jsonEncode({
    'success': true,
  }));
}
```

#### Step 3: Add CLI Commands

**File:** `bin/atomcad_cli.dart`

Add command handlers:

```dart
// In command dispatch section:

case 'load':
  await _handleLoad(args);
  break;

case 'save':
  await _handleSave(args);
  break;

case 'file':
  await _handleFile();
  break;

case 'new':
  await _handleNew(args);
  break;
```

**Command implementations:**

```dart
Future<void> _handleLoad(List<String> args) async {
  if (args.isEmpty) {
    print('Usage: load <path> [--force]');
    return;
  }

  final force = args.contains('--force');
  final path = args.where((a) => !a.startsWith('--')).first;
  final absolutePath = _resolvePath(path);

  final uri = Uri.parse('$baseUrl/load')
      .replace(queryParameters: {
        'path': absolutePath,
        if (force) 'force': 'true',
      });

  final response = await http.post(uri);
  final json = jsonDecode(response.body);

  if (json['success']) {
    print('Loaded: ${json['file_path']} (${json['network_count']} networks)');
  } else {
    print('Error: ${json['error']}');
  }
}

Future<void> _handleSave(List<String> args) async {
  final path = args.isNotEmpty ? args.first : null;

  final queryParams = <String, String>{};
  if (path != null) {
    queryParams['path'] = _resolvePath(path);
  }

  final uri = Uri.parse('$baseUrl/save').replace(queryParameters: queryParams);
  final response = await http.post(uri);
  final json = jsonDecode(response.body);

  if (json['success']) {
    print('Saved: ${json['file_path']}');
  } else {
    print('Error: ${json['error']}');
  }
}

Future<void> _handleFile() async {
  final uri = Uri.parse('$baseUrl/file');
  final response = await http.get(uri);
  final json = jsonDecode(response.body);

  if (json['success']) {
    final filePath = json['file_path'] ?? '(none)';
    final modified = json['modified'] ? 'yes' : 'no';
    final networkCount = json['network_count'];

    print('File: $filePath');
    print('Modified: $modified');
    print('Networks: $networkCount');
  } else {
    print('Error: ${json['error']}');
  }
}

Future<void> _handleNew(List<String> args) async {
  final force = args.contains('--force');

  final uri = Uri.parse('$baseUrl/new')
      .replace(queryParameters: {
        if (force) 'force': 'true',
      });

  final response = await http.post(uri);
  final json = jsonDecode(response.body);

  if (json['success']) {
    print('New project created.');
  } else {
    print('Error: ${json['error']}');
  }
}
```

#### Step 4: Update REPL Help

**File:** `bin/atomcad_cli.dart`

Add to help text:

```dart
  load <path> [--force]     Load a .cnnd file (--force discards unsaved changes)
  save [path]               Save to file (overwrites if path specified)
  file                      Show current file status
  new [--force]             Create new project (--force discards unsaved changes)
```

#### Step 5: Update Skill Documentation

**File:** `.claude/skills/atomcad/skill.md`

Add new section documenting the file commands:

```markdown
### File Operations

```bash
# Load a .cnnd file
atomcad-cli load design.cnnd
atomcad-cli load design.cnnd --force  # Discard unsaved changes

# Save current project
atomcad-cli save                      # Save to current file
atomcad-cli save design.cnnd          # Save to new path (overwrites if exists)

# Check file status
atomcad-cli file

# Create new project
atomcad-cli new
atomcad-cli new --force               # Discard unsaved changes
```
```

#### Step 6: Regenerate FFI Bindings

After adding new Rust API functions:

```bash
flutter_rust_bridge_codegen generate
```

---

## Testing Plan

### Unit Tests (Rust)

**File:** `rust/tests/unit/structure_designer_test.rs`

Tests for the `new_project()` method:
- `test_new_project_clears_networks`
- `test_new_project_creates_main_network`
- `test_new_project_clears_file_path`
- `test_new_project_clears_dirty_flag`

Note: The existing `load_node_networks()` and `save_node_networks()` methods are already covered by roundtrip tests in `rust/tests/integration/cnnd_roundtrip_test.rs`.

### Manual Testing Checklist

The CLI commands require a running atomCAD instance and are best tested manually:

1. [ ] Load a valid .cnnd file
2. [ ] Load with unsaved changes (should fail)
3. [ ] Load with --force and unsaved changes (should succeed)
4. [ ] Load nonexistent file (should fail with clear message)
5. [ ] Load corrupted file (should fail with clear message)
6. [ ] Save to new path
7. [ ] Save to current path after load
8. [ ] Save without file loaded (should fail)
9. [ ] Save overwrites existing file
10. [ ] File status with file loaded
11. [ ] File status without file loaded
12. [ ] File status shows correct modified state
13. [ ] New project clears everything
14. [ ] New with unsaved changes (should fail)
15. [ ] New with --force and unsaved changes (should succeed)

---

## File Summary

| File | Changes |
|------|---------|
| `rust/src/structure_designer/structure_designer.rs` | Add `new_project()` method |
| `rust/src/api/structure_designer/structure_designer_api.rs` | Add `new_project()` and `get_network_count()` API functions |
| `lib/ai_assistant/http_server.dart` | Add `/file`, `/load`, `/save`, `/new` endpoints |
| `bin/atomcad_cli.dart` | Add `load`, `save`, `file`, `new` command handlers |
| `.claude/skills/atomcad/skill.md` | Document new commands |

---

## Future Enhancements (Not in Scope)

- `file --info <path>` - Query metadata without loading
- Recent files list
- Auto-save functionality
- File format version display
